//! Assemble gathered + generated data into the final outputs:
//! a human-readable Markdown document, a token-efficient AI payload with an
//! instruction preamble, and individually-copiable sections.

use crate::config::Settings;
use crate::model::{JobData, OutputSection, Outputs};

fn fmt_duration(secs: f64) -> String {
    let s = secs as i64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

fn fmt_ts(secs: f64) -> String {
    fmt_duration(secs)
}

fn fmt_count(n: i64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.1}K", n as f64 / 1e3),
        n => n.to_string(),
    }
}

fn metadata_md(data: &JobData) -> String {
    let m = &data.meta;
    let mut s = String::new();
    if !m.channel.is_empty() {
        s.push_str(&format!("- **Channel:** {}\n", m.channel));
    }
    if m.duration > 0.0 {
        s.push_str(&format!("- **Duration:** {}\n", fmt_duration(m.duration)));
    }
    if m.view_count > 0 {
        s.push_str(&format!("- **Views:** {}\n", fmt_count(m.view_count)));
    }
    if m.like_count > 0 {
        s.push_str(&format!("- **Likes:** {}\n", fmt_count(m.like_count)));
    }
    if !m.upload_date.is_empty() && m.upload_date.len() == 8 {
        s.push_str(&format!(
            "- **Uploaded:** {}-{}-{}\n",
            &m.upload_date[0..4],
            &m.upload_date[4..6],
            &m.upload_date[6..8]
        ));
    }
    if !m.webpage_url.is_empty() {
        s.push_str(&format!("- **URL:** {}\n", m.webpage_url));
    }
    s
}

fn chapters_md(data: &JobData) -> String {
    let mut s = String::new();
    for c in &data.chapters {
        s.push_str(&format!("- `{}` {}\n", fmt_ts(c.start), c.title));
    }
    s
}

fn comments_md(data: &JobData) -> String {
    let mut s = String::new();
    for c in &data.comments {
        let pin = if c.is_favorited { " 📌" } else { "" };
        s.push_str(&format!(
            "- **{}** ({} likes){}: {}\n",
            c.author,
            fmt_count(c.likes),
            pin,
            c.text.replace('\n', " ")
        ));
    }
    s
}

fn transcript_md(data: &JobData, timestamps: bool) -> String {
    let mut s = String::new();
    if timestamps {
        for cue in &data.cues {
            s.push_str(&format!("[{}] {}\n", fmt_ts(cue.start), cue.text));
        }
    } else {
        // Flowing paragraph form.
        s.push_str(
            &data
                .cues
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
        s.push('\n');
    }
    s
}

/// The instruction preamble that tells an AI how to use the bundled data.
fn preamble(data: &JobData, settings: &Settings) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if settings.sections.ai_overview {
        parts.push("an AI-generated overview");
    }
    if settings.sections.visual_overview && !data.visual_overview.is_empty() {
        parts.push("a description of the media's visuals");
    }
    if settings.sections.comments && !data.comments.is_empty() {
        parts.push("top viewer comments");
    }
    if settings.sections.transcript && !data.cues.is_empty() {
        parts.push("the full transcript");
    }
    let contents = if parts.is_empty() {
        "media metadata".to_string()
    } else {
        parts.join(", ")
    };
    format!(
        "> **Instructions for AI:** The following is a structured export of a media source \
\"{title}\" — it contains {contents}. Treat the transcript as the primary source of truth \
about what was said, the visual overview as context for what was shown, and the comments as \
audience reaction (not fact). When answering questions about this media, ground your answers \
in the transcript and cite chapter timestamps where helpful. Do not fabricate details that \
are not present below.",
        title = data.meta.title,
        contents = contents,
    )
}

/// Build all outputs honoring the section toggles.
pub fn assemble(data: &JobData, settings: &Settings) -> Outputs {
    let sec = &settings.sections;
    let mut sections: Vec<OutputSection> = Vec::new();
    let push = |id: &str, title: &str, body: String, list: &mut Vec<OutputSection>| {
        if !body.trim().is_empty() {
            list.push(OutputSection {
                id: id.to_string(),
                title: title.to_string(),
                markdown: body,
            });
        }
    };

    if sec.metadata {
        push("metadata", "Details", metadata_md(data), &mut sections);
    }
    if sec.chapters && !data.chapters.is_empty() {
        push("chapters", "Chapters", chapters_md(data), &mut sections);
    }
    if sec.ai_overview && !data.ai_overview.is_empty() {
        push(
            "ai_overview",
            "AI Overview",
            data.ai_overview.clone(),
            &mut sections,
        );
    }
    if sec.visual_overview && !data.visual_overview.is_empty() {
        push(
            "visual_overview",
            "Visual Overview",
            data.visual_overview.clone(),
            &mut sections,
        );
    }
    if sec.comments && !data.comments.is_empty() {
        push("comments", "Top Comments", comments_md(data), &mut sections);
    }
    if sec.transcript && !data.cues.is_empty() {
        push(
            "transcript",
            "Transcript",
            transcript_md(data, settings.transcript_timestamps),
            &mut sections,
        );
    }

    let human_markdown = build_human(data, &sections);
    let ai_payload = build_ai(data, settings, &sections);

    Outputs {
        human_markdown,
        ai_payload,
        sections,
    }
}

fn build_human(data: &JobData, sections: &[OutputSection]) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {}\n\n", data.meta.title));
    for sec in sections {
        s.push_str(&format!(
            "## {}\n\n{}\n",
            sec.title,
            sec.markdown.trim_end()
        ));
        s.push('\n');
    }
    s.trim_end().to_string()
}

fn build_ai(data: &JobData, settings: &Settings, sections: &[OutputSection]) -> String {
    let mut s = String::new();
    if settings.sections.ai_preamble {
        s.push_str(&preamble(data, settings));
        s.push_str("\n\n");
    }
    s.push_str(&format!("# {}\n\n", data.meta.title));
    for sec in sections {
        s.push_str(&format!(
            "## {}\n\n{}\n\n",
            sec.title,
            sec.markdown.trim_end()
        ));
    }
    s.trim_end().to_string()
}
