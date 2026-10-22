//! OPML 2.0 import/export. Categories become Pulse tabs.

use crate::config::new_feed;
use crate::model::{Config, TabCfg};
use crate::util::slugify;
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Default)]
pub struct ImportResult {
    pub added: usize,
    pub skipped: usize,
    pub tabs_created: usize,
}

pub fn import(config: &mut Config, xml: &str) -> Result<ImportResult, String> {
    if xml.len() > 10 * 1024 * 1024 {
        return Err("OPML file is too large".into());
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<String> = Vec::new();
    let mut result = ImportResult::default();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                if event.name().as_ref() == b"outline" {
                    let attrs = outline_attrs(&event);
                    if let Some(url) = attrs.xml_url {
                        import_feed(
                            config,
                            &url,
                            &attrs.title,
                            attrs.html_url.as_deref(),
                            stack.last(),
                            &mut result,
                        );
                    } else if let Some(name) = attrs.text {
                        stack.push(name);
                    }
                }
            }
            Ok(Event::Empty(event)) => {
                if event.name().as_ref() == b"outline" {
                    let attrs = outline_attrs(&event);
                    if let Some(url) = attrs.xml_url {
                        import_feed(
                            config,
                            &url,
                            &attrs.title,
                            attrs.html_url.as_deref(),
                            stack.last(),
                            &mut result,
                        );
                    }
                }
            }
            Ok(Event::End(event)) => {
                if event.name().as_ref() == b"outline" {
                    stack.pop();
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("OPML parse error: {}", error)),
            _ => {}
        }
    }
    Ok(result)
}

#[derive(Default)]
struct OutlineAttrs {
    xml_url: Option<String>,
    html_url: Option<String>,
    text: Option<String>,
    title: String,
}

fn outline_attrs(event: &quick_xml::events::BytesStart) -> OutlineAttrs {
    let mut out = OutlineAttrs::default();
    for attr in event.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
        let value = attr
            .unescape_value()
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "xmlurl" => out.xml_url = Some(value),
            "htmlurl" => out.html_url = Some(value),
            "text" => {
                if out.title.is_empty() {
                    out.title = value.clone();
                }
                out.text = Some(value);
            }
            "title" => out.title = value,
            _ => {}
        }
    }
    out
}

fn import_feed(
    config: &mut Config,
    url: &str,
    title: &str,
    site_url: Option<&str>,
    category: Option<&String>,
    result: &mut ImportResult,
) {
    if !crate::util::is_http(url) {
        result.skipped += 1;
        return;
    }
    if let Some(existing) = config.feeds.iter().find(|f| f.url == url) {
        let existing_id = existing.id.clone();
        result.skipped += 1;
        if let Some(category) = category {
            assign_to_tab(config, &existing_id, category, result);
        }
        return;
    }

    let mut feed = new_feed(url, title);
    if let Some(site) = site_url {
        feed.site_url = site.to_string();
    }
    config.feeds.push(feed.clone());
    result.added += 1;
    if let Some(category) = category {
        assign_to_tab(config, &feed.id, category, result);
    }
}

fn assign_to_tab(config: &mut Config, feed_id: &str, category: &str, result: &mut ImportResult) {
    let name = category.trim();
    if name.is_empty() {
        return;
    }
    let id = slugify(name);
    if let Some(tab) = config.tabs.iter_mut().find(|t| t.id == id) {
        if !tab.feeds.iter().any(|f| f == feed_id) {
            tab.feeds.push(feed_id.to_string());
        }
        return;
    }
    config.tabs.push(TabCfg {
        id,
        name: name.to_string(),
        glyph: String::new(),
        feeds: vec![feed_id.to_string()],
        include: Vec::new(),
        exclude: Vec::new(),
        notify: true,
        images: None,
    });
    result.tabs_created += 1;
}

pub fn export(config: &Config) -> String {
    let mut body = String::new();
    let mut placed: Vec<&str> = Vec::new();

    for tab in &config.tabs {
        let feeds: Vec<_> = config
            .feeds
            .iter()
            .filter(|f| tab.feeds.iter().any(|id| id == &f.id))
            .collect();
        if feeds.is_empty() {
            continue;
        }
        body.push_str(&format!(
            "    <outline text=\"{}\" title=\"{}\">\n",
            escape(&tab.name),
            escape(&tab.name)
        ));
        for feed in feeds {
            if placed.contains(&feed.id.as_str()) {
                continue;
            }
            placed.push(feed.id.as_str());
            body.push_str(&feed_outline(feed, 6));
        }
        body.push_str("    </outline>\n");
    }

    let orphans: Vec<_> = config
        .feeds
        .iter()
        .filter(|f| !placed.contains(&f.id.as_str()))
        .collect();
    if !orphans.is_empty() {
        body.push_str("    <outline text=\"Pulse\" title=\"Pulse\">\n");
        for feed in orphans {
            body.push_str(&feed_outline(feed, 6));
        }
        body.push_str("    </outline>\n");
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">\n  <head>\n    <title>Pulse subscriptions</title>\n  </head>\n  <body>\n{}</body>\n</opml>\n",
        body
    )
}

fn feed_outline(feed: &crate::model::FeedCfg, indent: usize) -> String {
    let pad = " ".repeat(indent);
    format!(
        "{}<outline type=\"rss\" text=\"{}\" title=\"{}\" xmlUrl=\"{}\" htmlUrl=\"{}\"/>\n",
        pad,
        escape(&feed.title),
        escape(&feed.title),
        escape(&feed.url),
        escape(&feed.site_url)
    )
}

fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<opml version="2.0"><head><title>x</title></head><body>
  <outline text="Tech">
    <outline type="rss" text="Ars Technica" xmlUrl="https://example.com/ars.xml" htmlUrl="https://example.com"/>
  </outline>
  <outline type="rss" text="Loose Feed" xmlUrl="https://example.com/loose.xml"/>
</body></opml>"#;

    #[test]
    fn imports_feeds_and_categories() {
        let mut config = Config::default();
        let result = import(&mut config, SAMPLE).expect("import");
        assert_eq!(result.added, 2);
        assert_eq!(config.feeds.len(), 2);
        let tech = config
            .tabs
            .iter()
            .find(|t| t.name == "Tech")
            .expect("tech tab");
        assert_eq!(tech.feeds.len(), 1);
        assert_eq!(tech.id, "tech");
    }

    #[test]
    fn exports_round_trippable_opml() {
        let mut config = Config::default();
        import(&mut config, SAMPLE).expect("import");
        let exported = export(&config);
        assert!(exported.contains("Ars Technica"));
        assert!(exported.contains("xmlUrl=\"https://example.com/ars.xml\""));
        assert!(exported.contains("Loose Feed"));
        let mut second = Config::default();
        let result = import(&mut second, &exported).expect("re-import");
        assert_eq!(result.added, 2);
    }
}
