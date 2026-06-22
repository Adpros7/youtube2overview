//! Gemma 4 inference via the rapid-mlx OpenAI-compatible API: text overview + visual overview.

use anyhow::{anyhow, Context};
use base64::Engine;
use futures::stream::StreamExt;
use serde_json::{json, Value};

use crate::config::{OverviewLength, Settings};
use crate::mlx::Endpoint;
use crate::model::{Chapter, Cue, Frame, VideoMeta};

fn target_words(len: &OverviewLength) -> &'static str {
    match len {
        OverviewLength::Brief => "about 60-100 words",
        OverviewLength::Standard => "about 150-250 words",
        OverviewLength::Detailed => "about 350-500 words",
    }
}

fn lang_clause(settings: &Settings) -> String {
    let l = settings.language.trim();
    if l.is_empty() {
        String::new()
    } else {
        format!(" Respond in language code `{l}`.")
    }
}

/// Chunking thresholds for the map-reduce summarizer (chars of transcript text).
const SINGLE_PASS_LIMIT: usize = 24_000;
const CHUNK_CHARS: usize = 8_000;
/// Token budget for each per-chunk "map" summary (kept small; these are terse).
const MAP_MAX_TOKENS: u32 = 320;
/// How many per-chunk "map" summaries to have in flight at once. Bounded: they all hit
/// one server (the job's lease), so this mainly overlaps host-side gaps, not GPU compute.
const MAP_CONCURRENCY: usize = 3;

/// Low-level chat call honoring the configured `max_tokens`.
async fn chat(endpoint: &Endpoint, content: Value, settings: &Settings) -> anyhow::Result<String> {
    chat_inner(endpoint, content, settings, settings.max_tokens).await
}

/// Low-level chat call with an explicit token budget. `content` is the OpenAI message
/// content (string or array).
async fn chat_inner(
    endpoint: &Endpoint,
    content: Value,
    settings: &Settings,
    max_tokens: u32,
) -> anyhow::Result<String> {
    let body = json!({
        "model": endpoint.model_id,
        "messages": [{ "role": "user", "content": content }],
        "temperature": settings.temperature,
        "max_tokens": max_tokens,
        "stream": false,
        // Gemma 4 otherwise spends the whole token budget "thinking"; disable it so the
        // answer lands directly in `content`.
        "chat_template_kwargs": { "enable_thinking": false },
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/chat/completions", endpoint.base_url))
        .json(&body)
        .timeout(std::time::Duration::from_secs(60 * 10))
        .send()
        .await
        .context("inference request failed")?;
    let status = resp.status();
    let v: Value = resp.json().await.context("inference response not JSON")?;
    if !status.is_success() {
        return Err(anyhow!("inference error {status}: {v}"));
    }
    let text = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(anyhow!("empty completion"));
    }
    Ok(text)
}

/// Build a compact transcript string for prompting, capped at `max_chars`.
fn transcript_text(cues: &[Cue], max_chars: usize) -> String {
    let mut s = String::new();
    for c in cues {
        s.push_str(&c.text);
        s.push(' ');
        if s.len() >= max_chars {
            s.push_str("…[truncated]");
            break;
        }
    }
    s.trim().to_string()
}

/// Total transcript length in chars (cheap; used to choose single-pass vs. map-reduce).
fn transcript_len(cues: &[Cue]) -> usize {
    cues.iter().map(|c| c.text.len() + 1).sum()
}

/// `h:mm:ss` / `m:ss` timestamp for prompt labelling.
fn ts(secs: f64) -> String {
    let s = secs.max(0.0) as i64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

struct Chunk {
    start: f64,
    end: f64,
    text: String,
}

/// Partition cues into ~`chunk_chars`-sized windows at cue boundaries.
fn chunk_cues(cues: &[Cue], chunk_chars: usize) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    let mut start = cues.first().map(|c| c.start).unwrap_or(0.0);
    let mut last = start;
    for c in cues {
        last = c.start;
        cur.push_str(&c.text);
        cur.push(' ');
        if cur.len() >= chunk_chars {
            chunks.push(Chunk {
                start,
                end: c.start,
                text: std::mem::take(&mut cur).trim().to_string(),
            });
            start = c.start;
        }
    }
    if !cur.trim().is_empty() {
        chunks.push(Chunk {
            start,
            end: last,
            text: cur.trim().to_string(),
        });
    }
    chunks
}

fn chapters_clause(chapters: &[Chapter]) -> String {
    if chapters.is_empty() {
        return String::new();
    }
    let list = chapters
        .iter()
        .map(|c| format!("- {} ({})", c.title, ts(c.start)))
        .collect::<Vec<_>>()
        .join("\n");
    format!("\n\nChapters:\n{list}")
}

/// Generate the text overview from transcript + chapters.
///
/// Short transcripts go through a single call. Long ones are summarized map-reduce:
/// each ~`CHUNK_CHARS` window is summarized on its own (bounded, fast prompts that
/// cover the *whole* media item), then a final pass synthesizes those into the overview.
/// `progress(done, total)` is called as chunks complete (total == 1 for single-pass).
pub async fn text_overview(
    endpoint: &Endpoint,
    settings: &Settings,
    meta: &VideoMeta,
    chapters: &[Chapter],
    cues: &[Cue],
    progress: &(dyn Fn(usize, usize) + Send + Sync),
) -> anyhow::Result<String> {
    if cues.is_empty() {
        return Ok(String::new());
    }
    let chapters_str = chapters_clause(chapters);

    // Fast path: the whole transcript fits in one bounded prompt.
    if transcript_len(cues) <= SINGLE_PASS_LIMIT {
        progress(0, 1);
        let transcript = transcript_text(cues, SINGLE_PASS_LIMIT);
        let prompt = format!(
            "You are summarizing a media source for someone who has not watched or listened to it. \
Write a {len}, {style} overview based on the transcript. Capture the main topic, key \
points, and conclusion. Do not invent facts not present in the transcript. Use clear \
prose; you may use short bullet points for key takeaways.{lang}\n\n\
Title: {title}\nChannel: {channel}{chapters}\n\nTranscript:\n{transcript}",
            len = target_words(&settings.overview_length),
            style = settings.overview_style,
            lang = lang_clause(settings),
            title = meta.title,
            channel = meta.channel,
            chapters = chapters_str,
            transcript = transcript,
        );
        let out = chat(endpoint, json!(prompt), settings).await;
        progress(1, 1);
        return out;
    }

    // Map: summarize each window on its own. The per-chunk calls are independent, so we
    // fan them out (bounded) instead of awaiting one at a time — this fills the host-side
    // gaps between requests even when they land on the same server.
    let chunks = chunk_cues(cues, CHUNK_CHARS);
    let n = chunks.len();
    progress(0, n);
    // Owned per-chunk job tuples (index, start, end, prompt) so the concurrent futures
    // don't borrow `chunks` — avoids a higher-ranked-lifetime snag with `.map`.
    let jobs: Vec<(usize, f64, f64, String)> = chunks
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let prompt = format!(
                "This is part {idx}/{n} of a media transcript, covering {a}–{b}. \
Summarize the key points and any conclusions in 3–5 sentences. Only state what is \
present in the text; do not speculate.\n\nTranscript:\n{text}",
                idx = i + 1,
                n = n,
                a = ts(ch.start),
                b = ts(ch.end),
                text = ch.text,
            );
            (i, ch.start, ch.end, prompt)
        })
        .collect();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let done = &done;
    let mut indexed: Vec<(usize, String)> = futures::stream::iter(jobs)
        .map(|(i, start, end, prompt)| async move {
            let out = match chat_inner(endpoint, json!(prompt), settings, MAP_MAX_TOKENS).await {
                Ok(s) => Some((i, format!("[{}–{}] {}", ts(start), ts(end), s.trim()))),
                Err(e) => {
                    tracing::warn!("transcript chunk {} summary failed: {e:#}", i + 1);
                    None
                }
            };
            let completed = done.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            progress(completed, n);
            out
        })
        .buffer_unordered(MAP_CONCURRENCY)
        .filter_map(|r| async move { r })
        .collect()
        .await;
    if indexed.is_empty() {
        return Ok(String::new());
    }
    // Restore chronological order (buffer_unordered yields as each completes).
    indexed.sort_by_key(|(i, _)| *i);
    let summaries: Vec<String> = indexed.into_iter().map(|(_, s)| s).collect();

    // Reduce: synthesize the segment summaries into the final overview.
    let joined = summaries.join("\n\n");
    let reduce_prompt = format!(
        "You are summarizing a media source for someone who has not watched or listened to it. Below are \
ordered summaries of consecutive segments of the transcript (in time order). Synthesize \
them into a single {len}, {style} overview that captures the main topic, the key points as \
the media progresses, and the conclusion. Do not invent facts not present below. Use clear \
prose; you may use short bullet points for key takeaways.{lang}\n\n\
Title: {title}\nChannel: {channel}{chapters}\n\nSegment summaries:\n{joined}",
        len = target_words(&settings.overview_length),
        style = settings.overview_style,
        lang = lang_clause(settings),
        title = meta.title,
        channel = meta.channel,
        chapters = chapters_str,
        joined = joined,
    );
    chat(endpoint, json!(reduce_prompt), settings).await
}

/// Generate the visual overview from extracted keyframes (multimodal).
pub async fn visual_overview(
    endpoint: &Endpoint,
    settings: &Settings,
    meta: &VideoMeta,
    frames: &[Frame],
) -> anyhow::Result<String> {
    if frames.is_empty() {
        return Ok(String::new());
    }
    let mut content: Vec<Value> = Vec::new();
    let instruction = format!(
        "These are {n} keyframes sampled in chronological order from the media titled \
\"{title}\". Describe what is shown visually across it: setting, people, on-screen \
text, actions, and how the visuals progress. Be concrete and {style}. Write {len}.{lang}",
        n = frames.len(),
        title = meta.title,
        style = settings.overview_style,
        len = target_words(&settings.overview_length),
        lang = lang_clause(settings),
    );
    content.push(json!({ "type": "text", "text": instruction }));

    for f in frames {
        let Ok(bytes) = std::fs::read(&f.path) else {
            continue;
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        content.push(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:image/jpeg;base64,{b64}") }
        }));
    }

    chat(endpoint, json!(content), settings).await
}
