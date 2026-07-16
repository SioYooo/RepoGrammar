use super::{
    async_shape, leading_identifier, normalize_route_path, project_context, Anchor, AnchorOutcome,
    UnknownAnchor,
};
use crate::core::model::{CodeUnit, CodeUnitKind, SemanticFactKind, UnknownReasonCode};
use crate::ports::parser::{ParserProjectContext, SourceDocument};

const NEXT_HTTP_METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub(super) fn anchor(
    document: &SourceDocument<'_>,
    context: Option<&ParserProjectContext>,
    unit: &CodeUnit,
    slice: &str,
) -> AnchorOutcome {
    if !context.is_some_and(|context| project_context::has_package(context, "next")) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::MissingProjectConfig,
            affected_claim: "next_project_context",
            kind: "next_missing_package_context",
            note: "Next.js file convention requires package context",
        });
    }
    match unit.kind {
        CodeUnitKind::NextAppPage => next_component_anchor(
            "next.app.page",
            "next_app_page",
            "app",
            "page",
            slice,
            document.path,
        ),
        CodeUnitKind::NextAppLayout => next_component_anchor(
            "next.app.layout",
            "next_app_layout",
            "app",
            "layout",
            slice,
            document.path,
        ),
        CodeUnitKind::NextPagesPage => next_component_anchor(
            "next.pages.page",
            "next_pages_page",
            "pages",
            "page",
            slice,
            document.path,
        ),
        CodeUnitKind::NextPagesApiRoute => next_pages_api_route_anchor(slice, document.path),
        CodeUnitKind::NextRouteHandler => next_route_handler_anchor(slice, document.path),
        _ => AnchorOutcome::None,
    }
}

fn next_component_anchor(
    target: &'static str,
    anchor_kind: &'static str,
    router_kind: &'static str,
    file_convention: &'static str,
    slice: &str,
    path: &str,
) -> AnchorOutcome {
    if !slice.contains("export default") {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "next_default_export",
            kind: "next_reexported_page_unknown",
            note: "Next.js page/layout default export is not exact and local",
        });
    }
    let component_shape = if contains_jsx_like(slice) {
        "jsx_component"
    } else if slice.contains("createElement") {
        "create_element_component"
    } else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "next_component_shape",
            kind: "next_component_body_unknown",
            note: "Next.js page/layout component body is not an exact JSX/createElement anchor",
        });
    };
    AnchorOutcome::Anchor(Anchor {
        target: target.to_string(),
        fact_kind: SemanticFactKind::Symbol,
        assumptions: {
            let mut assumptions = vec![
                format!("tsjs_anchor_kind={anchor_kind}"),
                format!("router_kind={router_kind}"),
                format!("file_convention={file_convention}"),
                format!("route_path_shape={}", route_path_shape(path)),
                format!("component_shape={component_shape}"),
                "server_client_directive=unknown".to_string(),
            ];
            assumptions.extend(route_context_assumptions(path));
            assumptions
        },
    })
}

fn next_pages_api_route_anchor(slice: &str, path: &str) -> AnchorOutcome {
    if !slice.contains("export default") {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "next_pages_api_export",
            kind: "next_reexported_page_unknown",
            note: "Next.js Pages API route export is not exact and local",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: "next.pages.api_route".to_string(),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: {
            let mut assumptions = vec![
                "tsjs_anchor_kind=next_pages_api_route".to_string(),
                "router_kind=pages".to_string(),
                "file_convention=api_route".to_string(),
                format!("route_path_shape={}", route_path_shape(path)),
                format!("response_shape={}", response_shape(slice)),
                format!("async_shape={}", async_shape(slice)),
            ];
            assumptions.extend(route_context_assumptions(path));
            assumptions
        },
    })
}

fn next_route_handler_anchor(slice: &str, path: &str) -> AnchorOutcome {
    let Some(method) = route_method(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "next_route_handler_export",
            kind: "next_route_handler_export_unknown",
            note: "Next.js route handler export is not an exact HTTP method function",
        });
    };
    AnchorOutcome::Anchor(Anchor {
        target: format!("next.route.{method}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: {
            let mut assumptions = vec![
                "tsjs_anchor_kind=next_route_handler".to_string(),
                "router_kind=app".to_string(),
                "file_convention=route".to_string(),
                format!("http_method={method}"),
                format!("route_path_shape={}", route_path_shape(path)),
                format!("response_shape={}", response_shape(slice)),
                format!("fetch_shape={}", fetch_shape(slice)),
                format!("async_shape={}", async_shape(slice)),
                "server_client_directive=server_assumed".to_string(),
            ];
            assumptions.extend(route_context_assumptions(path));
            assumptions
        },
    })
}

fn route_context_assumptions(path: &str) -> Vec<String> {
    vec![
        format!("dynamic_segment_present={}", path.contains("/[")),
        format!("route_group_present={}", path.contains("/(")),
        format!("parallel_route_present={}", path.contains("/@")),
    ]
}

fn route_path_shape(path: &str) -> String {
    let mut path = path.trim_start_matches("./").to_string();
    for extension in [".tsx", ".jsx", ".ts", ".js"] {
        if path.ends_with(extension) {
            path.truncate(path.len() - extension.len());
            break;
        }
    }
    for suffix in ["/page", "/layout", "/route"] {
        if path.ends_with(suffix) {
            path.truncate(path.len() - suffix.len());
            break;
        }
    }
    if let Some(rest) = path.strip_prefix("app/") {
        normalize_route_path(&format!("/{rest}"))
    } else if let Some(rest) = path.strip_prefix("src/app/") {
        normalize_route_path(&format!("/{rest}"))
    } else if let Some(rest) = path.strip_prefix("pages/api/") {
        normalize_route_path(&format!("/api/{rest}"))
    } else if let Some(rest) = path.strip_prefix("pages/") {
        normalize_route_path(&format!("/{rest}"))
    } else {
        normalize_route_path(&format!("/{path}"))
    }
}

fn route_method(slice: &str) -> Option<&'static str> {
    NEXT_HTTP_METHODS.iter().copied().find(|method| {
        slice.contains(&format!("function {method}"))
            || exported_const_async_route_handler(slice, method)
    })
}

fn exported_const_async_route_handler(slice: &str, method: &str) -> bool {
    let trimmed = slice.trim_start();
    let Some(after_export) = trimmed.strip_prefix("export ") else {
        return false;
    };
    let Some(after_const) = after_export.trim_start().strip_prefix("const ") else {
        return false;
    };
    let Some((name, after_name)) = leading_identifier(after_const) else {
        return false;
    };
    if name != method {
        return false;
    }
    let Some(rhs) = after_const[after_name..]
        .trim_start()
        .strip_prefix('=')
        .map(str::trim_start)
    else {
        return false;
    };
    rhs.starts_with("async ") && rhs.contains("=>")
}

fn contains_jsx_like(slice: &str) -> bool {
    slice.contains("return <")
        || slice.contains("</")
        || slice.contains("/>")
        || slice.contains("jsx(")
}

fn response_shape(slice: &str) -> &'static str {
    if slice.contains("NextResponse.json") || slice.contains("Response.json") {
        "response_json"
    } else if slice.contains("new Response") {
        "response_object"
    } else if slice.contains(".json(") {
        "res_json"
    } else if slice.contains(".send(") {
        "res_send"
    } else if slice.contains(".end(") {
        "res_end"
    } else {
        "response_unknown"
    }
}

fn fetch_shape(slice: &str) -> &'static str {
    if slice.contains("request.json(") {
        "request_json"
    } else if slice.contains("request.nextUrl") {
        "next_url"
    } else {
        "none"
    }
}
