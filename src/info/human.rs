use super::report::*;
use std::fmt::Write;

pub fn render(r: &InfoReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{} · {}", r.id, r.kind);
    if let Some(d) = &r.description {
        let _ = writeln!(s, "{d}");
    }
    let _ = writeln!(s);

    kv(&mut s, "ID", &r.id);
    kv(&mut s, "Kind", &r.kind);
    if let Some(t) = &r.title {
        kv(&mut s, "Title", t);
    }
    if !r.tags.is_empty() {
        kv(&mut s, "Tags", &r.tags.join(", "));
    }

    let resolve_line = match r.resolve.status.as_str() {
        "bound" => format!(
            "bound · sidecar {}",
            r.resolve.sidecar_path.as_deref().unwrap_or("")
        ),
        "partial" => format!(
            "partial · {}/{} nodes resolved",
            r.resolve.resolved_nodes, r.resolve.total_nodes
        ),
        _ => "unbound".to_string(),
    };
    kv(&mut s, "Resolve", &resolve_line);

    if !r.entrypoints.is_empty() {
        let _ = writeln!(s, "\nEntrypoints ({})", r.entrypoints.len());
        for e in &r.entrypoints {
            let _ = writeln!(s, "  {} → {}", e.name, e.target);
        }
    }
    if !r.nodes.is_empty() {
        let _ = writeln!(s, "\nNodes ({})", r.nodes.len());
        for n in &r.nodes {
            let mut extra = String::new();
            if let Some(op) = &n.operation {
                extra.push_str(&format!(" · op={op}"));
            }
            if let Some(pa) = &n.pack_alias {
                extra.push_str(&format!(" · pack={pa}"));
            }
            let _ = writeln!(s, "  {:<15} {}{}", n.id, n.component_id, extra);
        }
    }
    if !r.parameters.is_empty() {
        let _ = writeln!(s, "\nParameters ({})", r.parameters.len());
        for p in &r.parameters {
            let opt = if p.required { "" } else { "?" };
            let _ = writeln!(s, "  {:<15} {}{}", p.name, p.ty, opt);
        }
    }
    s
}

fn kv(s: &mut String, label: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    let _ = writeln!(s, "{:<12} {}", label, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> InfoReport {
        InfoReport {
            info_schema_version: 1,
            id: "weather-bot".into(),
            kind: "messaging".into(),
            title: Some("Weather bot".into()),
            description: Some("Returns the weather.".into()),
            tags: vec!["demo".into(), "weather".into()],
            resolve: ResolveStatus {
                status: "bound".into(),
                sidecar_path: Some("weather-bot.ygtc.resolve.json".into()),
                resolved_nodes: 2,
                total_nodes: 2,
            },
            entrypoints: vec![EntrypointInfo {
                name: "on-message".into(),
                target: "ask-city".into(),
            }],
            nodes: vec![
                NodeInfo {
                    id: "ask-city".into(),
                    component_id: "questions".into(),
                    operation: None,
                    pack_alias: None,
                    routing: "Next(\"fetch-weather\")".into(),
                },
                NodeInfo {
                    id: "fetch-weather".into(),
                    component_id: "component.exec".into(),
                    operation: Some("weather.fetch".into()),
                    pack_alias: Some("weather".into()),
                    routing: "End".into(),
                },
            ],
            parameters: vec![ParameterInfo {
                name: "units".into(),
                ty: "string".into(),
                required: true,
            }],
        }
    }

    #[test]
    fn renders_bound_flow_header_and_sections() {
        let out = render(&sample());
        assert!(out.contains("weather-bot · messaging"));
        assert!(out.contains("Returns the weather."));
        assert!(out.contains("bound · sidecar weather-bot.ygtc.resolve.json"));
        assert!(out.contains("on-message → ask-city"));
        assert!(out.contains("ask-city"));
        assert!(out.contains("fetch-weather"));
        assert!(out.contains("op=weather.fetch"));
        assert!(out.contains("pack=weather"));
        assert!(out.contains("units           string"));
    }

    #[test]
    fn renders_unbound_flow() {
        let mut r = sample();
        r.resolve = ResolveStatus {
            status: "unbound".into(),
            sidecar_path: None,
            resolved_nodes: 0,
            total_nodes: 2,
        };
        let out = render(&r);
        assert!(out.contains("Resolve"));
        assert!(out.contains("unbound"));
    }

    #[test]
    fn renders_partial_resolve() {
        let mut r = sample();
        r.resolve = ResolveStatus {
            status: "partial".into(),
            sidecar_path: Some("x.resolve.json".into()),
            resolved_nodes: 1,
            total_nodes: 3,
        };
        let out = render(&r);
        assert!(out.contains("partial · 1/3 nodes resolved"));
    }
}
