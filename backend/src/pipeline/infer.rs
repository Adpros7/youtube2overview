//! Gemma 4 inference via the rapid-mlx OpenAI-compatible API: text overview + visual overview.

use anyhow::{anyhow, Context};
use base64::Engine;
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

/// Low-level chat call. `content` is the OpenAI message content (string or array).
async fn chat(endpoint: &Endpoint, content: Value, settings: &Settings) -> anyhow::Result<String> {
    let body = json!({
        "model": endpoint.model_id,
        "messages": [{ "role": "user", "content": content }],
        "temperature": settings.temperature,
        "max_tokens": settings.max_tokens,
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

/// Build a compact transcript string for prompting.
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

/// Generate the text overview from transcript + chapters.
pub async fn text_overview(
    endpoint: &Endpoint,
    settings: &Settings,
    meta: &VideoMeta,
    chapters: &[Chapter],
    cues: &[Cue],
) -> anyhow::Result<String> {
    let transcript = transcript_text(cues, 120_000);
    if transcript.is_empty() {
        return Ok(String::new());
    }
    let chapters_str = if chapters.is_empty() {
        String::new()
    } else {
        let list = chapters
            .iter()
            .map(|c| format!("- {} ({:.0}s)", c.title, c.start))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n\nChapters:\n{list}")
    };

    let prompt = format!(
        "You are summarizing a YouTube video for someone who has not watched it. \
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

    chat(endpoint, json!(prompt), settings).await
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
        "These are {n} keyframes sampled in chronological order from the video titled \
\"{title}\". Describe what is shown visually across the video: setting, people, on-screen \
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
