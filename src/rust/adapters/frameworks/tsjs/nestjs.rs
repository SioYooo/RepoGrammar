pub(crate) const ROLE_CONTROLLER: &str = "framework:nestjs.controller";
pub(crate) const ROLE_ROUTE: &str = "framework:nestjs.route";
pub(crate) const ROLE_INJECTABLE: &str = "framework:nestjs.injectable";
pub(crate) const ROLE_MODULE: &str = "framework:nestjs.module";

pub(crate) const TARGET_CONTROLLER: &str = "nestjs.common.Controller";
pub(crate) const TARGET_INJECTABLE: &str = "nestjs.common.Injectable";
pub(crate) const TARGET_MODULE: &str = "nestjs.common.Module";

pub(crate) const ROUTE_TARGETS: &[&str] = &[
    "nestjs.common.Get",
    "nestjs.common.Post",
    "nestjs.common.Put",
    "nestjs.common.Delete",
    "nestjs.common.Patch",
    "nestjs.common.Head",
    "nestjs.common.Options",
    "nestjs.common.All",
];
