pub(crate) const ROLE_APP_PAGE: &str = "framework:next.app.page";
pub(crate) const ROLE_APP_LAYOUT: &str = "framework:next.app.layout";
pub(crate) const ROLE_ROUTE_HANDLER: &str = "framework:next.route.handler";
pub(crate) const ROLE_PAGES_API_ROUTE: &str = "framework:next.pages.api_route";
pub(crate) const ROLE_PAGES_PAGE: &str = "framework:next.pages.page";

pub(crate) const TARGET_APP_PAGE: &str = "next.app.page";
pub(crate) const TARGET_APP_LAYOUT: &str = "next.app.layout";
pub(crate) const TARGET_PAGES_API_ROUTE: &str = "next.pages.api_route";
pub(crate) const TARGET_PAGES_PAGE: &str = "next.pages.page";

pub(crate) const ROUTE_HANDLER_TARGETS: &[&str] = &[
    "next.route.GET",
    "next.route.POST",
    "next.route.PUT",
    "next.route.PATCH",
    "next.route.DELETE",
    "next.route.HEAD",
    "next.route.OPTIONS",
];
