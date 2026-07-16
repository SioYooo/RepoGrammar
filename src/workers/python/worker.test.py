#!/usr/bin/env python3
"""Smoke tests for the dependency-free Python worker."""

from __future__ import annotations

import ast
import hashlib
import json
import os
import runpy
import subprocess
import sys
import tempfile
from pathlib import Path

WORKER = Path(__file__).with_name("worker.py")


def run_worker(payload):
    data = payload if isinstance(payload, str) else json.dumps(payload) + "\n"
    result = subprocess.run(
        [sys.executable, str(WORKER)],
        input=data,
        text=True,
        capture_output=True,
        check=False,
        timeout=30,
    )
    assert result.returncode == 0, result.stderr
    assert result.stderr == ""
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


def valid_request(root: str):
    return {
        "protocol_version": 1,
        "request_id": "repogrammar-python-semantic-worker",
        "project_root": root,
        "changed_files": ["app.py"],
    }


def assert_end_of_stream(messages):
    assert messages[-1] == {
        "protocol_version": 1,
        "message_type": "end_of_stream",
        "request_id": "repogrammar-python-semantic-worker",
    }


def has_unknown_for_subject(facts, subject_name, reason_code, affected_claim):
    return any(
        fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == reason_code
        and subject_name in fact["subject"]
        and f"affected_claim={affected_claim}" in fact["assumptions"]
        for fact in facts
    )


def assert_no_fact_source_payloads(facts):
    forbidden = {"source", "source_text", "snippet", "source_snippet"}

    def walk(value):
        if isinstance(value, dict):
            assert forbidden.isdisjoint(value)
            for child in value.values():
                walk(child)
        elif isinstance(value, list):
            for child in value:
                walk(child)

    for fact in facts:
        walk(fact)


parse_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "app.py",
        "content_hash": "sha256:" + "0" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from fastapi import APIRouter, Body, Cookie, Depends, Header, HTTPException, Path, Query
from app.services import UserService, run_query
from pydantic import BaseModel, ConfigDict, Field, computed_field, field_validator, model_validator, validator
from sqlalchemy import select, text
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column, relationship
from typing import Annotated
import pytest
import pytest as pt
from pytest import fixture as pytest_fixture

router = APIRouter()

class UserOut(BaseModel):
    model_config: ConfigDict = ConfigDict(from_attributes=True)
    id: int
    display_name: str = Field(default="", min_length=1)

    @field_validator("id")
    @classmethod
    def validate_id(cls, value):
        return value

    @validator("display_name")
    @classmethod
    def validate_display_name(cls, value):
        return value

    @computed_field
    @property
    def label(self) -> str:
        return self.display_name

    @model_validator(mode="after")
    def validate_model(self):
        return self

    class Config:
        arbitrary_types_allowed = True

class Base(DeclarativeBase):
    pass

class Account(Base):
    id: Mapped[int] = mapped_column(primary_key=True)

class User(Base):
    id: Mapped[int] = mapped_column(primary_key=True)
    accounts = relationship("Account")

class UserRepository:
    def list_users(self, session: Session):
        session.add(User())
        return session.execute("select users")

    def select_users(self, session: Session):
        return session.execute(select(User))

    def raw_text_users(self, session: Session):
        return session.execute(text("select users"))

    def get_user(self, session: Session):
        return session.scalar("select user")

    def stream_users(self, session: Session):
        return session.scalars("select users")

    def load_user(self, session: Session):
        return session.get(User, 1)

    async def list_accounts(self, db: AsyncSession):
        return await db.execute("select accounts")

    async def get_account(self, db: AsyncSession):
        return await db.scalar("select account")

    async def stream_accounts(self, db: AsyncSession):
        return await db.scalars("select accounts")

    async def load_account(self, db: AsyncSession):
        return await db.get(User, 1)

class StoredSessionRepository:
    def __init__(self, session: Session, db: AsyncSession):
        self.session = session
        self.db: AsyncSession = db

    def commit_users(self):
        self.session.commit()
        return self.session.execute("select users")

    def rollback_users(self):
        self.session.rollback()

    async def commit_accounts(self):
        await self.db.commit()

    async def rollback_accounts(self):
        await self.db.rollback()

def get_db():
    return object()

def read_users():
    service = UserService()
    alias = service
    return alias.list_users()

def read_products():
    service = UserService()
    service = object()
    return service.list_products()

def run_imported():
    runner = run_query
    return runner()

@router.get("/users/{user_id}", response_model=list[UserOut])
async def list_users(
    user_id: int = Path(...),
    payload: Annotated[UserOut, Body()] = None,
    query: str = Query(""),
    request_id: str = Header(""),
    session_id: str = Cookie(""),
    dependency=Depends(get_db),
):
    service = UserService()
    alias = service
    getattr(alias, "dynamic_users")()
    if False:
        raise HTTPException(status_code=404)
    return alias.list_users()

@router.get("/products")
def list_products():
    return run_query()

@router.get("/orders")
def list_orders():
    service = UserService()
    service = object()
    return service.list_orders()

def raises_not_found():
    raise HTTPException(status_code=404)

@pytest_fixture
def client():
    return object()

@pt.fixture
def db():
    return object()

@pytest.mark.parametrize("status", [200])
def test_users(client, status, missing_fixture):
    assert client.get("/users").status_code == status
""",
    }
)
assert len(parse_messages) == 1
unit_kinds = [unit["kind"] for unit in parse_messages[0]["units"]]
assert "module" in unit_kinds
assert "fastapi_route" in unit_kinds
assert "pytest_test" in unit_kinds
assert sum(1 for kind in unit_kinds if kind == "pytest_fixture") == 2
assert "pydantic_model" in unit_kinds
assert "sqlalchemy_model" in unit_kinds
assert "async_function" not in unit_kinds
generic_unit_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "generic_units.py",
        "content_hash": "sha256:" + "6" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
def helper():
    return 1

async def fetch():
    return 2

class Plain:
    def method(self):
        return helper()

    async def async_method(self):
        return await fetch()
""",
    }
)
generic_unit_kinds = [unit["kind"] for unit in generic_unit_messages[0]["units"]]
assert "module" in generic_unit_kinds
assert "function" in generic_unit_kinds
assert "async_function" in generic_unit_kinds
assert "class" in generic_unit_kinds
assert sum(1 for kind in generic_unit_kinds if kind == "method") == 2
assert not any(
    kind
    in {
        "fastapi_route",
        "pytest_test",
        "pytest_fixture",
        "pydantic_model",
        "sqlalchemy_model",
        "sqlalchemy_repository_method",
    }
    for kind in generic_unit_kinds
)
non_sqlalchemy_get_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "cache_repository.py",
        "content_hash": "sha256:" + "b" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
class CacheRepository:
    def read_cache(self, cache):
        return cache.get("users")
""",
    }
)
non_sqlalchemy_get_kinds = [unit["kind"] for unit in non_sqlalchemy_get_messages[0]["units"]]
assert "sqlalchemy_repository_method" not in non_sqlalchemy_get_kinds
parse_facts = parse_messages[0]["facts"]
assert any(fact["fact_kind"] == "RESOLVED_IMPORT" and fact["target"] == "fastapi.APIRouter" for fact in parse_facts)
assert any(fact["fact_kind"] == "SYMBOL" and fact["target"] == "app" for fact in parse_facts)
assert any(fact["fact_kind"] == "SYMBOL" and fact["target"] == "scope.imported.APIRouter" for fact in parse_facts)
assert any(fact["fact_kind"] == "SYMBOL" and fact["target"] == "scope.namespace.UserOut" for fact in parse_facts)
assert any(fact["fact_kind"] == "SYMBOL" and fact["target"] == "scope.assigned.router" for fact in parse_facts)
assert any(fact["fact_kind"] == "TYPE" and fact["target"] == "pydantic.BaseModel" for fact in parse_facts)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pydantic.field.id"
    and "python_anchor_kind=pydantic_field" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "TYPE"
    and fact["target"] == "pydantic.field_type.int"
    and "python_anchor_kind=pydantic_field_type" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "pydantic.Field"
    and "python_anchor_kind=pydantic_field_metadata" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pydantic.model_config"
    and "python_anchor_kind=pydantic_model_config" in fact["assumptions"]
    for fact in parse_facts
)
assert not any(fact.get("target") == "pydantic.field.model_config" for fact in parse_facts)
assert not any(fact.get("target") == "pydantic.field_type.pydantic.ConfigDict" for fact in parse_facts)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pydantic.Config"
    and "python_anchor_kind=pydantic_config_class" in fact["assumptions"]
    for fact in parse_facts
)
assert any(fact["fact_kind"] == "TYPE" and fact["target"] == "sqlalchemy.orm.DeclarativeBase" for fact in parse_facts)
assert any(fact["fact_kind"] == "TYPE" and fact["target"] == "sqlalchemy.orm.Mapped" for fact in parse_facts)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL" and fact["target"] == "sqlalchemy.orm.mapped_column"
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.orm.relationship"
    and "python_anchor_kind=sqlalchemy_relationship" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "sqlalchemy.relationship_target.Account"
    and "python_anchor_kind=sqlalchemy_relationship_target" in fact["assumptions"]
    and "fact_scope=context_only" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL" and fact["target"] == "sqlalchemy.orm.Session.add"
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL" and fact["target"] == "sqlalchemy.orm.Session.execute"
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "FrameworkMagic"
    and "affected_claim=sqlalchemy_query_shape" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.orm.Session.scalar"
    and "python_anchor_kind=sqlalchemy_session_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.orm.Session.scalars"
    and "python_anchor_kind=sqlalchemy_session_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.orm.Session.get"
    and "python_anchor_kind=sqlalchemy_session_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.ext.asyncio.AsyncSession.execute"
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.ext.asyncio.AsyncSession.scalar"
    and "python_anchor_kind=sqlalchemy_session_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.ext.asyncio.AsyncSession.scalars"
    and "python_anchor_kind=sqlalchemy_session_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.ext.asyncio.AsyncSession.get"
    and "python_anchor_kind=sqlalchemy_session_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.orm.Session.commit"
    and "python_anchor_kind=sqlalchemy_session_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.orm.Session.rollback"
    and "python_anchor_kind=sqlalchemy_session_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.ext.asyncio.AsyncSession.commit"
    and "python_anchor_kind=sqlalchemy_session_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.ext.asyncio.AsyncSession.rollback"
    and "python_anchor_kind=sqlalchemy_session_call" in fact["assumptions"]
    for fact in parse_facts
)

dynamic_relationship_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "dynamic_relationship.py",
        "content_hash": "sha256:" + "e" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from sqlalchemy.orm import DeclarativeBase, relationship

class Base(DeclarativeBase):
    pass

def resolve_target():
    return "Account"

class User(Base):
    accounts = relationship(resolve_target())

class AuditLog(Base):
    owner = relationship("ExternalAccount")
""",
    }
)
dynamic_relationship_facts = dynamic_relationship_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "FrameworkMagic"
    and "affected_claim=sqlalchemy_relationship_target" in fact["assumptions"]
    for fact in dynamic_relationship_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "UnresolvedImport"
    and "affected_claim=sqlalchemy_relationship_target" in fact["assumptions"]
    for fact in dynamic_relationship_facts
)
assert not any(
    fact.get("target") == "sqlalchemy.relationship_target.Account"
    for fact in dynamic_relationship_facts
)
assert "resolve_target()" not in json.dumps(dynamic_relationship_messages)
assert "select users" not in json.dumps(parse_messages)

structured_select_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "structured_select.py",
        "content_hash": "sha256:" + "e" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from sqlalchemy import select
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    id: Mapped[int] = mapped_column(primary_key=True)

class UserRepository:
    def list_users(self, session: Session):
        return session.execute(select(User))
""",
    }
)
structured_select_facts = structured_select_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.orm.Session.execute"
    for fact in structured_select_facts
)
assert not any(
    fact["fact_kind"] == "UNKNOWN"
    and "affected_claim=sqlalchemy_query_shape" in fact["assumptions"]
    for fact in structured_select_facts
)

assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "app.services.UserService.list_users"
    and "python_anchor_kind=fastapi_service_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "app.services.run_query"
    and "python_anchor_kind=fastapi_service_call" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "app.services.UserService.list_users"
    and "python_anchor_kind=call_target" in fact["assumptions"]
    for fact in parse_facts
)
assert not any(fact.get("target") == "app.services.UserService.list_orders" for fact in parse_facts)
assert not any(
    fact.get("target") == "service.list_orders"
    and "python_anchor_kind=fastapi_service_call" in fact.get("assumptions", [])
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "FrameworkMagic"
    and "affected_claim=python_call_target" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.APIRouter.get"
    and "python_anchor_kind=fastapi_route_decorator" in fact["assumptions"]
    for fact in parse_facts
)
route_methods = ["delete", "get", "head", "options", "patch", "post", "put"]
route_matrix_source = "\n".join(
    [
        "from fastapi import APIRouter, FastAPI",
        "router = APIRouter()",
        "app = FastAPI()",
        "",
        *[
            f"@router.{method}('/router-{method}')\n"
            f"def router_{method}():\n"
            f"    return {{}}\n"
            for method in route_methods
        ],
        *[
            f"@app.{method}('/app-{method}')\n"
            f"def app_{method}():\n"
            f"    return {{}}\n"
            for method in route_methods
        ],
    ]
)
route_matrix_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "routes.py",
        "content_hash": "sha256:" + "7" * 64,
        "repository_revision": "UNKNOWN",
        "text": route_matrix_source,
    }
)
route_matrix_facts = route_matrix_messages[0]["facts"]
route_matrix_targets = {
    fact["target"]
    for fact in route_matrix_facts
    if fact["fact_kind"] == "SYMBOL"
    and "python_anchor_kind=fastapi_route_decorator" in fact["assumptions"]
}
assert route_matrix_targets == {
    *(f"fastapi.APIRouter.{method}" for method in route_methods),
    *(f"fastapi.FastAPI.{method}" for method in route_methods),
}
route_matrix_unit_kinds = [unit["kind"] for unit in route_matrix_messages[0]["units"]]
assert route_matrix_unit_kinds.count("fastapi_route") == len(route_methods) * 2
serialized_route_matrix = json.dumps(route_matrix_messages)
assert "@router." not in serialized_route_matrix
assert "@app." not in serialized_route_matrix

local_client_route_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "client_route_false_positive.py",
        "content_hash": "sha256:" + "8" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
client = object()

@client.get("/users")
def not_a_fastapi_route():
    return {}
""",
    }
)
local_client_units = local_client_route_messages[0]["units"]
local_client_facts = local_client_route_messages[0]["facts"]
assert not any(unit["kind"] == "fastapi_route" for unit in local_client_units)
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"].startswith("fastapi.")
    and "python_anchor_kind=fastapi_route_decorator" in fact["assumptions"]
    for fact in local_client_facts
)

range_shadow_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "range_shadow_routes.py",
        "content_hash": "sha256:" + "8" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from fastapi import APIRouter

router = APIRouter()

@router.get("/before")
def before_shadow():
    return {}

router = object()

@router.get("/after")
def after_shadow():
    return {}
""",
    }
)
range_shadow_units = range_shadow_messages[0]["units"]
range_shadow_facts = range_shadow_messages[0]["facts"]
assert sum(1 for unit in range_shadow_units if unit["kind"] == "fastapi_route") == 1
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.APIRouter.get"
    and "before_shadow" in fact["subject"]
    for fact in range_shadow_facts
)
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.APIRouter.get"
    and "after_shadow" in fact["subject"]
    for fact in range_shadow_facts
)
assert any(
    fact["fact_kind"] == "TYPE"
    and fact["target"] == "fastapi.response_model.UserOut"
    and "python_anchor_kind=fastapi_response_model" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "TYPE"
    and fact["target"] == "fastapi.request_body.UserOut"
    and "python_anchor_kind=fastapi_request_body_model" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.request_param.path.user_id"
    and "python_anchor_kind=fastapi_path_param" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.request_param.query.query"
    and "python_anchor_kind=fastapi_query_param" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.request_param.header.request_id"
    and "python_anchor_kind=fastapi_header_param" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.request_param.cookie.session_id"
    and "python_anchor_kind=fastapi_cookie_param" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "fastapi.Depends"
    and "python_anchor_kind=fastapi_dependency" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.dependency.get_db"
    and "python_anchor_kind=fastapi_dependency_target" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "fastapi.HTTPException"
    and "python_anchor_kind=fastapi_http_exception" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.http_exception.status_code.404"
    and "python_anchor_kind=fastapi_http_exception_status" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pydantic.field_validator"
    and "python_anchor_kind=pydantic_validator" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pydantic.validator"
    and "python_anchor_kind=pydantic_validator" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pydantic.computed_field"
    and "python_anchor_kind=pydantic_computed_field" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pydantic.model_validator"
    and "python_anchor_kind=pydantic_model_validator" in fact["assumptions"]
    for fact in parse_facts
)

dynamic_pydantic_model_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "models.py",
        "content_hash": "sha256:" + "d" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from pydantic import create_model
import pydantic as pyd

DynamicUser = create_model("DynamicUser", secret=(str, ...))
DynamicOrder = pyd.create_model("DynamicOrder", amount=(int, ...))
""",
    }
)
dynamic_pydantic_model_facts = dynamic_pydantic_model_messages[0]["facts"]
assert (
    sum(
        1
        for fact in dynamic_pydantic_model_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "FrameworkMagic"
        and "affected_claim=python_framework_identity" in fact["assumptions"]
    )
    == 2
)
assert not any(
    fact["fact_kind"] == "RESOLVED_CALL" and fact["target"] == "pydantic.create_model"
    for fact in dynamic_pydantic_model_facts
)
serialized_dynamic_pydantic_models = json.dumps(dynamic_pydantic_model_messages)
assert "secret=(str" not in serialized_dynamic_pydantic_models
dynamic_pydantic_config_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "dynamic_config.py",
        "content_hash": "sha256:" + "f" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from pydantic import BaseModel, ConfigDict

class DynamicConfigModel(BaseModel):
    model_config = ConfigDict(extra=policy())
    id: int

class StaticConfigModel(BaseModel):
    model_config = ConfigDict(from_attributes=True, extra="ignore")
    id: int
""",
    }
)
dynamic_pydantic_config_facts = dynamic_pydantic_config_messages[0]["facts"]
assert (
    sum(
        1
        for fact in dynamic_pydantic_config_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "FrameworkMagic"
        and "affected_claim=python_framework_identity" in fact["assumptions"]
    )
    == 1
)
assert (
    sum(
        1
        for fact in dynamic_pydantic_config_facts
        if fact["fact_kind"] == "SYMBOL"
        and fact["target"] == "pydantic.model_config"
        and "python_anchor_kind=pydantic_model_config" in fact["assumptions"]
    )
    == 2
)
assert "policy()" not in json.dumps(dynamic_pydantic_config_messages)

pydantic_validator_side_effect_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "validator_side_effects.py",
        "content_hash": "sha256:" + "f" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from pydantic import BaseModel, field_validator, model_validator

class User(BaseModel):
    name: str

    @field_validator("name")
    @classmethod
    def normalize_name(cls, value):
        audit.write(value)
        return value

    @model_validator(mode="after")
    def audit_model(self):
        sink(self)
        return self
""",
    }
)
pydantic_validator_side_effect_facts = pydantic_validator_side_effect_messages[0]["facts"]
assert (
    sum(
        1
        for fact in pydantic_validator_side_effect_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "FrameworkMagic"
        and "affected_claim=pydantic_validator_side_effects" in fact["assumptions"]
    )
    == 2
)
assert any(
    fact["fact_kind"] == "TYPE" and fact["target"] == "pydantic.BaseModel"
    for fact in pydantic_validator_side_effect_facts
)
serialized_validator_side_effects = json.dumps(pydantic_validator_side_effect_messages)
assert "audit.write(value)" not in serialized_validator_side_effects
assert "sink(self)" not in serialized_validator_side_effects

assert "min_length" not in json.dumps(parse_messages)

assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.mark.parametrize"
    and "python_anchor_kind=pytest_parametrize" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture"
    and "python_anchor_kind=pytest_fixture_decorator" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture.client"
    and "python_anchor_kind=pytest_fixture_edge" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.parametrize.status"
    and "python_anchor_kind=pytest_parametrize_arg" in fact["assumptions"]
    for fact in parse_facts
)
assert any(fact["fact_kind"] == "RESOLVED_CALL" and fact["target"] == "client.get" for fact in parse_facts)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.test"
    and "python_anchor_kind=pytest_test_function" in fact["assumptions"]
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "PytestFixtureInjection"
    and "affected_claim=pytest_fixture_binding" in fact["assumptions"]
    for fact in parse_facts
)
for fact in parse_facts:
    assert fact["origin"]["engine"] == "python"
    assert fact["origin"]["method"] == "cpython_ast"
    assert fact["certainty"] in {"STRUCTURAL", "DATAFLOW_DERIVED", "UNKNOWN"}
    if fact["certainty"] == "DATAFLOW_DERIVED":
        assert "provider_resolved=false" in fact["assumptions"]
        assert any(
            assumption
            in {
                "derived_from=repo_local_python_import_graph",
                "derived_from=repo_local_pytest_fixture_graph",
            }
            for assumption in fact["assumptions"]
        )
    assert fact["evidence"]["path"] == "app.py"
    assert fact["evidence"]["content_hash"] == "sha256:" + "0" * 64
    assert "start_byte" in fact["evidence"]
    assert "end_byte" in fact["evidence"]
assert "from fastapi" not in json.dumps(parse_messages)
assert "Body()" not in json.dumps(parse_messages)
assert "Path(" not in json.dumps(parse_messages)
assert "Query(" not in json.dumps(parse_messages)
assert "Header(" not in json.dumps(parse_messages)
assert "Cookie(" not in json.dumps(parse_messages)
assert "model_config =" not in json.dumps(parse_messages)
assert "arbitrary_types_allowed" not in json.dumps(parse_messages)
assert "dynamic_users" not in json.dumps(parse_messages)
assert "@router.get" not in json.dumps(parse_messages)
assert "response_model=" not in json.dumps(parse_messages)
assert "list[UserOut]" not in json.dumps(parse_messages)
assert "Depends(" not in json.dumps(parse_messages)
assert "Depends(get_db" not in json.dumps(parse_messages)
assert "HTTPException(" not in json.dumps(parse_messages)

runtime_session_injection_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "runtime_repository.py",
        "content_hash": "sha256:" + "d" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
class UserRepository:
    def __init__(self, session):
        self.session = session

    def list_users(self):
        return self.session.execute("select users")
""",
    }
)
runtime_session_injection_facts = runtime_session_injection_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "RuntimeDependencyInjection"
    and "affected_claim=python_framework_identity" in fact["assumptions"]
    for fact in runtime_session_injection_facts
)
assert not any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.orm.Session.execute"
    for fact in runtime_session_injection_facts
)
assert "select users" not in json.dumps(runtime_session_injection_messages)

sqlalchemy_event_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "events.py",
        "content_hash": "sha256:" + "d" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from sqlalchemy import event

class User:
    pass

def audit(mapper, connection, target):
    pass

event.listen(User, "before_insert", audit)

@event.listens_for(User, "after_update")
def receive_update(mapper, connection, target):
    pass
""",
    }
)
sqlalchemy_event_facts = sqlalchemy_event_messages[0]["facts"]
assert (
    sum(
        1
        for fact in sqlalchemy_event_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "FrameworkMagic"
        and "affected_claim=python_framework_identity" in fact["assumptions"]
    )
    >= 2
)
assert not any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] in {"sqlalchemy.event.listen", "sqlalchemy.event.listens_for"}
    for fact in sqlalchemy_event_facts
)
assert "before_insert" not in json.dumps(sqlalchemy_event_messages)

dynamic_sqlalchemy_model_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "dynamic_sqlalchemy_model.py",
        "content_hash": "sha256:" + "d" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from sqlalchemy.orm import declarative_base

Base = declarative_base()
DynamicUser = type("DynamicUser", (Base,), {"__tablename__": "users"})
Plain = type("Plain", (object,), {})
""",
    }
)
dynamic_sqlalchemy_model_facts = dynamic_sqlalchemy_model_messages[0]["facts"]
assert (
    sum(
        1
        for fact in dynamic_sqlalchemy_model_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "FrameworkMagic"
        and "affected_claim=python_framework_identity" in fact["assumptions"]
    )
    == 1
)
assert "sqlalchemy_model" not in [unit["kind"] for unit in dynamic_sqlalchemy_model_messages[0]["units"]]
assert "__tablename__" not in json.dumps(dynamic_sqlalchemy_model_messages)

sqlalchemy_query_wrapper_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "query_wrappers.py",
        "content_hash": "sha256:" + "d" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from sqlalchemy import select
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    id: Mapped[int] = mapped_column(primary_key=True)

def execute_users(session: Session):
    return session.execute(select(User))

class UserRepository:
    def _execute_users(self, session: Session):
        return session.execute(select(User))

    def list_users(self, session: Session):
        return execute_users(session)

    def list_wrapped_users(self, session: Session):
        return self._execute_users(session)
""",
    }
)
sqlalchemy_query_wrapper_facts = sqlalchemy_query_wrapper_messages[0]["facts"]
assert (
    sum(
        1
        for fact in sqlalchemy_query_wrapper_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "FrameworkMagic"
        and "affected_claim=python_framework_identity" in fact["assumptions"]
    )
    == 2
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.orm.Session.execute"
    for fact in sqlalchemy_query_wrapper_facts
)
assert not any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] in {"execute_users", "self._execute_users"}
    for fact in sqlalchemy_query_wrapper_facts
)
serialized_query_wrappers = json.dumps(sqlalchemy_query_wrapper_messages)
assert "return execute_users(session)" not in serialized_query_wrappers
assert "return self._execute_users(session)" not in serialized_query_wrappers

shadowed_session_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "repository.py",
        "content_hash": "sha256:" + "d" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from sqlalchemy.orm import Session

class UserRepository:
    def __init__(self, session: Session):
        self.session = session

    def list_users(self):
        self.session = object()
        return self.session.execute("select users")
""",
    }
)
shadowed_session_facts = shadowed_session_messages[0]["facts"]
assert not any(
    fact["fact_kind"] == "RESOLVED_CALL"
    and fact["target"] == "sqlalchemy.orm.Session.execute"
    for fact in shadowed_session_facts
)
assert "return self.session.execute" not in json.dumps(shadowed_session_messages)

indirect_parametrize_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "test_indirect.py",
        "content_hash": "sha256:" + "9" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
import pytest

@pytest.mark.parametrize("client,status", [("api", 200)], indirect=["client"])
def test_indirect_list(client, status):
    assert status == 200

@pytest.mark.parametrize("resource", ["db"], indirect=True)
def test_indirect_all(resource):
    assert resource
""",
    }
)
assert len(indirect_parametrize_messages) == 1
indirect_facts = indirect_parametrize_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.parametrize.status"
    and "python_anchor_kind=pytest_parametrize_arg" in fact["assumptions"]
    for fact in indirect_facts
)
assert not any(
    fact["fact_kind"] == "SYMBOL" and fact["target"] == "pytest.parametrize.client"
    for fact in indirect_facts
)
assert not any(
    fact["fact_kind"] == "SYMBOL" and fact["target"] == "pytest.parametrize.resource"
    for fact in indirect_facts
)
assert sum(
    1
    for fact in indirect_facts
    if fact["fact_kind"] == "UNKNOWN" and fact["target"] == "PytestFixtureInjection"
) >= 2

parametrize_fixture_collision_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "test_parametrize_collision.py",
        "content_hash": "sha256:" + "e" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
import pytest

@pytest.fixture
def client():
    return object()

@pytest.mark.parametrize("client", ["api"])
def test_direct_client(client):
    assert client

@pytest.mark.parametrize("db", ["db"], indirect=True)
def test_indirect_db(db):
    assert db
""",
    }
)
collision_facts = parametrize_fixture_collision_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.parametrize.client"
    and "python_anchor_kind=pytest_parametrize_arg" in fact["assumptions"]
    for fact in collision_facts
)
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture.client"
    and "python_anchor_kind=pytest_fixture_edge" in fact["assumptions"]
    for fact in collision_facts
)
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.parametrize.db"
    for fact in collision_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "PytestFixtureInjection"
    and "affected_claim=pytest_fixture_binding" in fact["assumptions"]
    for fact in collision_facts
)

settings_parse_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "settings.py",
        "content_hash": "sha256:" + "c" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from pydantic import BaseSettings as LegacyBaseSettings
from pydantic_settings import BaseSettings


class LegacySettings(LegacyBaseSettings):
    debug: bool = False


class AppSettings(BaseSettings):
    debug: bool = False
""",
    }
)
settings_units = settings_parse_messages[0]["units"]
settings_facts = settings_parse_messages[0]["facts"]
assert sum(1 for unit in settings_units if unit["kind"] == "pydantic_model") == 2
assert any(
    fact["fact_kind"] == "TYPE" and fact["target"] == "pydantic.BaseSettings"
    for fact in settings_facts
)
assert any(
    fact["fact_kind"] == "TYPE" and fact["target"] == "pydantic_settings.BaseSettings"
    for fact in settings_facts
)
assert "from pydantic import" not in json.dumps(settings_parse_messages)

aliased_framework_model_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "aliased_models.py",
        "content_hash": "sha256:" + "5" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from pydantic import BaseModel as BM
from sqlalchemy.orm import Mapped as M, mapped_column as col

class UserOut(BM):
    id: int

class User:
    id: M[int] = col(primary_key=True)
""",
    }
)
aliased_framework_units = aliased_framework_model_messages[0]["units"]
aliased_framework_facts = aliased_framework_model_messages[0]["facts"]
assert any(unit["kind"] == "pydantic_model" for unit in aliased_framework_units)
assert any(unit["kind"] == "sqlalchemy_model" for unit in aliased_framework_units)
assert any(
    fact["fact_kind"] == "TYPE" and fact["target"] == "pydantic.BaseModel"
    for fact in aliased_framework_facts
)
assert any(
    fact["fact_kind"] == "TYPE" and fact["target"] == "sqlalchemy.orm.Mapped"
    for fact in aliased_framework_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL" and fact["target"] == "sqlalchemy.orm.mapped_column"
    for fact in aliased_framework_facts
)

declarative_base_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "declarative_base_models.py",
        "content_hash": "sha256:" + "5" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from sqlalchemy.orm import declarative_base

Base = declarative_base()

class User(Base):
    __tablename__ = "users"
""",
    }
)
declarative_base_units = declarative_base_messages[0]["units"]
declarative_base_facts = declarative_base_messages[0]["facts"]
assert any(unit["kind"] == "sqlalchemy_model" for unit in declarative_base_units)
assert any(
    fact["fact_kind"] == "TYPE"
    and fact["target"] == "sqlalchemy.orm.declarative_base"
    and "python_anchor_kind=class_base" in fact["assumptions"]
    for fact in declarative_base_facts
)
assert "Base = declarative_base()" not in json.dumps(declarative_base_messages)

local_base_model_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "local_base_model.py",
        "content_hash": "sha256:" + "5" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
class BaseModel:
    pass

class UserOut(BaseModel):
    id: int

class Base:
    pass

class User(Base):
    __tablename__ = "users"
""",
    }
)
local_base_units = local_base_model_messages[0]["units"]
local_base_facts = local_base_model_messages[0]["facts"]
assert not any(unit["kind"] == "pydantic_model" for unit in local_base_units)
assert not any(unit["kind"] == "sqlalchemy_model" for unit in local_base_units)
assert not any(fact.get("target") == "pydantic.BaseModel" for fact in local_base_facts)
assert not any(fact.get("target") == "sqlalchemy.orm.DeclarativeBase" for fact in local_base_facts)

external_framework_base_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "external_base_model.py",
        "content_hash": "sha256:" + "5" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from app.db import Base
from app.schemas import BaseSchema
from pydantic import Field
from sqlalchemy.orm import Mapped, mapped_column

class UserOut(BaseSchema):
    id: int = Field(default=0)

class User(Base):
    __tablename__ = "users"
    id: Mapped[int] = mapped_column(primary_key=True)
""",
    }
)
external_framework_base_facts = external_framework_base_messages[0]["facts"]
assert (
    sum(
        1
        for fact in external_framework_base_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "FrameworkMagic"
        and "affected_claim=python_framework_identity" in fact["assumptions"]
    )
    == 2
)
assert not any(
    unit["name"] == "UserOut" and unit["kind"] == "pydantic_model"
    for unit in external_framework_base_messages[0]["units"]
)
assert any(
    unit["name"] == "User" and unit["kind"] == "sqlalchemy_model"
    for unit in external_framework_base_messages[0]["units"]
)
serialized_external_base = json.dumps(external_framework_base_messages)
assert "Field(default=0)" not in serialized_external_base
assert "__tablename__ = " not in serialized_external_base

alias_parse_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "alias_routes.py",
        "content_hash": "sha256:" + "a" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from fastapi import APIRouter, Depends, HTTPException

router = APIRouter()
api = router
v1 = api

@v1.get("/users")
def list_users():
    return []

@v1.get("/dynamic", response_model=make_response_model())
def dynamic_response_model(dependency=Depends(make_dependency())):
    raise HTTPException(status_code=make_status())
    return []
""",
    }
)
alias_units = alias_parse_messages[0]["units"]
alias_facts = alias_parse_messages[0]["facts"]
assert any(unit["kind"] == "fastapi_route" for unit in alias_units)
assert any(
    fact["fact_kind"] == "SYMBOL" and fact["target"] == "fastapi.APIRouter.get"
    for fact in alias_facts
)
assert not any(
    fact["fact_kind"] == "TYPE"
    and "python_anchor_kind=fastapi_response_model" in fact["assumptions"]
    for fact in alias_facts
)
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and "python_anchor_kind=fastapi_dependency_target" in fact["assumptions"]
    for fact in alias_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "RuntimeDependencyInjection"
    and "affected_claim=fastapi_dependency_target" in fact["assumptions"]
    for fact in alias_facts
)
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and "python_anchor_kind=fastapi_http_exception_status" in fact["assumptions"]
    for fact in alias_facts
)
assert "@v1.get" not in json.dumps(alias_parse_messages)
assert "response_model=" not in json.dumps(alias_parse_messages)
assert "Depends(make_dependency" not in json.dumps(alias_parse_messages)
assert "HTTPException(" not in json.dumps(alias_parse_messages)

dependency_unknown_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "dependency_unknowns.py",
        "content_hash": "sha256:" + "c" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from fastapi import APIRouter, Depends

router = APIRouter()

def make_dependency():
    return object()

@router.get("/call")
def call_dependency(current_user=Depends(make_dependency())):
    return {}

@router.get("/lambda")
def lambda_dependency(current_user=Depends(lambda: object())):
    return {}

@router.get("/conditional")
def conditional_dependency(current_user=Depends(make_dependency if True else None)):
    return {}

@router.get("/missing")
def missing_dependency(current_user=Depends(missing_dep)):
    return {}

@router.get("/attribute")
def attribute_dependency(current_user=Depends(plugins.current_user)):
    return {}

@router.get("/empty")
def empty_dependency(current_user=Depends()):
    return {}
""",
    }
)
dependency_unknown_facts = dependency_unknown_messages[0]["facts"]
dependency_unknowns = [
    fact
    for fact in dependency_unknown_facts
    if fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "RuntimeDependencyInjection"
    and "affected_claim=fastapi_dependency_target" in fact["assumptions"]
]
assert len(dependency_unknowns) == 5
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and "python_anchor_kind=fastapi_dependency_target" in fact["assumptions"]
    for fact in dependency_unknown_facts
)
assert sum(
    1
    for fact in dependency_unknown_facts
    if fact["fact_kind"] == "RESOLVED_CALL" and fact["target"] == "fastapi.Depends"
) == 6
serialized_dependency_unknowns = json.dumps(dependency_unknown_messages)
assert "Depends(make_dependency" not in serialized_dependency_unknowns
assert "lambda: object" not in serialized_dependency_unknowns
assert "plugins.current_user" not in serialized_dependency_unknowns

shadowed_alias_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "shadowed_routes.py",
        "content_hash": "sha256:" + "b" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from fastapi import APIRouter

router = APIRouter()
api = router
api = object()

@api.get("/users")
def list_users():
    return []
""",
    }
)
shadowed_facts = shadowed_alias_messages[0]["facts"]
assert not any(
    fact["fact_kind"] == "SYMBOL" and fact["target"] == "fastapi.APIRouter.get"
    for fact in shadowed_facts
)
assert "@api.get" not in json.dumps(shadowed_alias_messages)

shadowed_framework_import_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "shadowed_framework_imports.py",
        "content_hash": "sha256:" + "f" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from fastapi import APIRouter
from pydantic import BaseModel
from pytest import fixture
from sqlalchemy.orm import Mapped, declarative_base, mapped_column

APIRouter = object
BaseModel = object
fixture = object
declarative_base = object
Mapped = list
mapped_column = object

router = APIRouter()

@router.get("/users")
def list_users():
    return []

class UserOut(BaseModel):
    id: int

@fixture
def client():
    return object()

Base = declarative_base()

class LegacyUser(Base):
    __tablename__ = "legacy_users"

class User:
    __tablename__ = "users"
    id: Mapped[int] = mapped_column()
""",
    }
)
shadowed_framework_import_units = shadowed_framework_import_messages[0]["units"]
shadowed_framework_import_facts = shadowed_framework_import_messages[0]["facts"]
assert not any(unit["kind"] == "sqlalchemy_model" for unit in shadowed_framework_import_units)
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.APIRouter.get"
    and "python_anchor_kind=fastapi_route_decorator" in fact["assumptions"]
    for fact in shadowed_framework_import_facts
)
assert not any(
    fact["fact_kind"] == "TYPE" and fact["target"] == "pydantic.BaseModel"
    for fact in shadowed_framework_import_facts
)
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture"
    and "python_anchor_kind=pytest_fixture_decorator" in fact["assumptions"]
    for fact in shadowed_framework_import_facts
)
assert not any(
    (
        fact["fact_kind"] == "TYPE"
        and fact["target"] == "sqlalchemy.orm.Mapped"
        and "python_anchor_kind=sqlalchemy_mapped_field" in fact["assumptions"]
    )
    or (
        fact["fact_kind"] == "RESOLVED_CALL"
        and fact["target"] == "sqlalchemy.orm.mapped_column"
        and "python_anchor_kind=sqlalchemy_mapped_column" in fact["assumptions"]
    )
    or (
        fact["fact_kind"] == "TYPE"
        and fact["target"] == "sqlalchemy.orm.declarative_base"
        and "python_anchor_kind=class_base" in fact["assumptions"]
    )
    for fact in shadowed_framework_import_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "FrameworkMagic"
    and "affected_claim=python_framework_identity" in fact["assumptions"]
    for fact in shadowed_framework_import_facts
)

module_dynamic_boundary_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "module_dynamic_boundary.py",
        "content_hash": "sha256:" + "a" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
import importlib
import sys
from fastapi import APIRouter

sys.path.insert(0, "/tmp/secret")
importlib.import_module("plugins.dynamic")

router = APIRouter()

@router.get("/users")
def list_users():
    return []
""",
    }
)
module_dynamic_boundary_facts = module_dynamic_boundary_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "fastapi.APIRouter.get"
    and "list_users" in fact["subject"]
    for fact in module_dynamic_boundary_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "RuntimeDependencyInjection"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    and "list_users" in fact["subject"]
    for fact in module_dynamic_boundary_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "DynamicImport"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    and "list_users" in fact["subject"]
    for fact in module_dynamic_boundary_facts
)
serialized_module_dynamic_boundary = json.dumps(module_dynamic_boundary_messages)
assert "/tmp/secret" not in serialized_module_dynamic_boundary
assert "plugins.dynamic" not in serialized_module_dynamic_boundary

module_dynamic_after_route_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "module_dynamic_after_route.py",
        "content_hash": "sha256:" + "a" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
import importlib
from fastapi import APIRouter

router = APIRouter()

@router.get("/before")
def before_dynamic():
    return {}

importlib.import_module("plugins.dynamic")

@router.get("/after")
def after_dynamic():
    return {}
""",
    }
)
module_dynamic_after_route_facts = module_dynamic_after_route_messages[0]["facts"]
assert not any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "DynamicImport"
    and "before_dynamic" in fact["subject"]
    for fact in module_dynamic_after_route_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "DynamicImport"
    and "after_dynamic" in fact["subject"]
    for fact in module_dynamic_after_route_facts
)

bad_parse = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "broken.py",
        "content_hash": "sha256:" + "1" * 64,
        "repository_revision": "UNKNOWN",
        "text": "def broken(:\n",
    }
)
assert bad_parse[0]["units"] == []
assert bad_parse[0]["diagnostics"][0]["message"] == "python ast parse failed"
assert bad_parse[0]["facts"] == []

dynamic_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "dynamic.py",
        "content_hash": "sha256:" + "2" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
import importlib
import sys

def load(name, obj, method):
    importlib.import_module(name)
    sys.path.append("/tmp/secret")
    getattr(obj, method)()
    globals()[name]()
    locals()[name]()
    eval("/tmp/secret")
    exec("/tmp/secret")
    compile("/tmp/secret", "/tmp/secret", "exec")
    __import__(name)
    setattr(obj, method, object())

def decorator_factory(name):
    def inner(function):
        return function
    return inner

@decorator_factory("secret")
def decorated():
    return None
""",
    }
)
dynamic_facts = dynamic_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "DynamicImport"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    for fact in dynamic_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "FrameworkMagic"
    and "affected_claim=python_call_target" in fact["assumptions"]
    for fact in dynamic_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "MonkeyPatch"
    and "affected_claim=python_call_target" in fact["assumptions"]
    for fact in dynamic_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "FrameworkMagic"
    and "affected_claim=python_framework_identity" in fact["assumptions"]
    for fact in dynamic_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "RuntimeDependencyInjection"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    for fact in dynamic_facts
)
assert "importlib.import_module(name)" not in json.dumps(dynamic_messages)
assert "locals()[name]" not in json.dumps(dynamic_messages)
assert "eval(\"/tmp/secret\")" not in json.dumps(dynamic_messages)
assert "exec(\"/tmp/secret\")" not in json.dumps(dynamic_messages)
assert "compile(\"/tmp/secret\"" not in json.dumps(dynamic_messages)
assert "__import__(name)" not in json.dumps(dynamic_messages)
assert "decorator_factory(\"secret\")" not in json.dumps(dynamic_messages)
assert "setattr(obj" not in json.dumps(dynamic_messages)
assert "/tmp/secret" not in json.dumps(dynamic_messages)
assert_no_fact_source_payloads(dynamic_facts)

typed_dynamic_boundary_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "typed_dynamic_boundaries.py",
        "content_hash": "sha256:" + "2" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
import importlib
import sys

def dynamic_importlib(name):
    importlib.import_module(name)

def dynamic_builtin_import(name):
    __import__(name)

def dynamic_locals(name):
    locals()[name]()

def dynamic_globals(name):
    globals()[name]()

def dynamic_eval(expr):
    eval(expr)

def dynamic_exec(expr):
    exec(expr)

def dynamic_compile(expr):
    compile(expr, "generated.py", "exec")

def dynamic_getattr(obj, method):
    getattr(obj, method)()

def dynamic_path(extra_path):
    sys.path.append(extra_path)

def dynamic_setattr(obj, name):
    setattr(obj, name, object())
""",
    }
)
typed_dynamic_boundary_facts = typed_dynamic_boundary_messages[0]["facts"]
assert has_unknown_for_subject(
    typed_dynamic_boundary_facts,
    "dynamic_importlib",
    "DynamicImport",
    "python_import_resolution",
)
assert has_unknown_for_subject(
    typed_dynamic_boundary_facts,
    "dynamic_builtin_import",
    "DynamicImport",
    "python_import_resolution",
)
assert has_unknown_for_subject(
    typed_dynamic_boundary_facts,
    "dynamic_locals",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    typed_dynamic_boundary_facts,
    "dynamic_globals",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    typed_dynamic_boundary_facts,
    "dynamic_eval",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    typed_dynamic_boundary_facts,
    "dynamic_exec",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    typed_dynamic_boundary_facts,
    "dynamic_compile",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    typed_dynamic_boundary_facts,
    "dynamic_getattr",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    typed_dynamic_boundary_facts,
    "dynamic_path",
    "RuntimeDependencyInjection",
    "python_import_resolution",
)
assert has_unknown_for_subject(
    typed_dynamic_boundary_facts,
    "dynamic_setattr",
    "MonkeyPatch",
    "python_call_target",
)
assert_no_fact_source_payloads(typed_dynamic_boundary_facts)
serialized_typed_dynamic_boundaries = json.dumps(typed_dynamic_boundary_messages)
assert "locals()[name]" not in serialized_typed_dynamic_boundaries
assert "globals()[name]" not in serialized_typed_dynamic_boundaries
assert "eval(expr)" not in serialized_typed_dynamic_boundaries
assert "exec(expr)" not in serialized_typed_dynamic_boundaries
assert "compile(expr" not in serialized_typed_dynamic_boundaries
assert "__import__(name)" not in serialized_typed_dynamic_boundaries
assert "generated.py" not in serialized_typed_dynamic_boundaries

additional_dynamic_call_form_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "additional_dynamic_forms.py",
        "content_hash": "sha256:" + "2" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": ["additional_dynamic_forms.py", "plugins/safe.py"],
        "text": """
import importlib
from importlib import import_module as load_module

module_loader = importlib.import_module
module_scope = globals()
module_patch = setattr

def alias_nonliteral(name):
    loader = importlib.import_module
    return loader(name)

def alias_literal():
    loader = importlib.import_module
    return loader("plugins.safe")

def imported_alias_nonliteral(name):
    return load_module(name)

def namespace_alias(name):
    scope = globals()
    scope[name]()

def namespace_get_call(name):
    globals().get(name)()

def dynamic_lookup_alias(obj, method):
    handler = getattr(obj, method)
    handler()

def monkey_patch_alias(obj, name):
    patch = setattr
    patch(obj, name, object())

def execution_alias(expr):
    runner = eval
    runner(expr)

def module_alias_import(name):
    return module_loader(name)

def module_namespace_alias(name):
    module_scope[name]()

def module_patch_alias(obj, name):
    module_patch(obj, name, object())
""",
    }
)
additional_dynamic_call_form_facts = additional_dynamic_call_form_messages[0]["facts"]
assert has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "alias_nonliteral",
    "DynamicImport",
    "python_import_resolution",
)
assert any(
    fact["fact_kind"] == "RESOLVED_IMPORT"
    and fact["target"] == "plugins.safe"
    and "alias_literal" in fact["subject"]
    and "python_anchor_kind=dynamic_import_literal" in fact["assumptions"]
    for fact in additional_dynamic_call_form_facts
)
assert not has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "alias_literal",
    "DynamicImport",
    "python_import_resolution",
)
assert has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "imported_alias_nonliteral",
    "DynamicImport",
    "python_import_resolution",
)
assert has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "namespace_alias",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "namespace_get_call",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "dynamic_lookup_alias",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "monkey_patch_alias",
    "MonkeyPatch",
    "python_call_target",
)
assert has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "execution_alias",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "module_alias_import",
    "DynamicImport",
    "python_import_resolution",
)
assert has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "module_namespace_alias",
    "FrameworkMagic",
    "python_call_target",
)
assert has_unknown_for_subject(
    additional_dynamic_call_form_facts,
    "module_patch_alias",
    "MonkeyPatch",
    "python_call_target",
)
assert_no_fact_source_payloads(additional_dynamic_call_form_facts)
serialized_additional_dynamic_forms = json.dumps(additional_dynamic_call_form_messages)
assert "loader(name)" not in serialized_additional_dynamic_forms
assert "scope[name]" not in serialized_additional_dynamic_forms
assert "globals().get" not in serialized_additional_dynamic_forms
assert "getattr(obj" not in serialized_additional_dynamic_forms
assert "module_loader(name)" not in serialized_additional_dynamic_forms

bare_dynamic_lookup_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "bare_dynamic_lookup.py",
        "content_hash": "sha256:" + "2" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
def inspect_scope():
    globals()
    locals()
""",
    }
)
bare_dynamic_lookup_facts = bare_dynamic_lookup_messages[0]["facts"]
assert (
    sum(
        1
        for fact in bare_dynamic_lookup_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "FrameworkMagic"
        and "affected_claim=python_call_target" in fact["assumptions"]
    )
    == 2
)

unresolved_decorator_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "decorators.py",
        "content_hash": "sha256:" + "e" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
def local_decorator(function):
    return function

@local_decorator
def local_view():
    return {}

@unknown_policy
def protected_view():
    return {}

class Resource:
    @property
    def label(self):
        return "resource"
""",
    }
)
unresolved_decorator_facts = unresolved_decorator_messages[0]["facts"]
assert (
    sum(
        1
        for fact in unresolved_decorator_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "FrameworkMagic"
        and "affected_claim=python_framework_identity" in fact["assumptions"]
    )
    == 1
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "unknown_policy"
    and "python_anchor_kind=decorator_binding" in fact["assumptions"]
    for fact in unresolved_decorator_facts
)
serialized_unresolved_decorator = json.dumps(unresolved_decorator_messages)
assert "return function" not in serialized_unresolved_decorator
assert "return \"resource\"" not in serialized_unresolved_decorator

fixture_dependency_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "tests/test_fixture_graph.py",
        "content_hash": "sha256:" + "f" * 64,
        "repository_revision": "UNKNOWN",
        "conftest_files": [
            {
                "path": "tests/conftest.py",
                "text": """
import pytest

@pytest.fixture
def external_user():
    return object()
""",
            }
        ],
        "text": """
import pytest

fixture_alias = pytest.fixture

@pytest.fixture
def db():
    return object()

@fixture_alias(name="api_client")
def client(db, external_user, tmp_path, missing_fixture):
    return object()

def helper(db):
    return db

def test_users(api_client):
    assert api_client

def test_literal_lookup(request):
    assert request.getfixturevalue("api_client")

def test_dynamic_lookup(request, fixture_name):
    assert request.getfixturevalue(fixture_name)
""",
    }
)
fixture_dependency_facts = fixture_dependency_messages[0]["facts"]
assert sum(
    1
    for fact in fixture_dependency_facts
    if fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture.db"
    and "python_anchor_kind=pytest_fixture_edge" in fact["assumptions"]
) == 1
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture.external_user"
    and "python_anchor_kind=pytest_conftest_fixture_edge" in fact["assumptions"]
    for fact in fixture_dependency_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.builtin_fixture.tmp_path"
    and "python_anchor_kind=pytest_builtin_fixture_context" in fact["assumptions"]
    for fact in fixture_dependency_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture.api_client"
    and fact["certainty"] == "DATAFLOW_DERIVED"
    and "python_anchor_kind=pytest_fixture_edge" in fact["assumptions"]
    and "derived_from=repo_local_pytest_fixture_graph" in fact["assumptions"]
    for fact in fixture_dependency_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "PytestFixtureInjection"
    and "affected_claim=pytest_fixture_binding" in fact["assumptions"]
    for fact in fixture_dependency_facts
)
serialized_fixture_dependency = json.dumps(fixture_dependency_messages)
assert "return object" not in serialized_fixture_dependency
assert "missing_fixture" not in serialized_fixture_dependency
assert "tests/conftest.py" not in serialized_fixture_dependency

unsafe_literal_import = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "unsafe_import.py",
        "content_hash": "sha256:" + "3" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
import importlib

def load():
    importlib.import_module("/tmp/secret")
""",
    }
)
serialized_unsafe_import = json.dumps(unsafe_literal_import)
assert "/tmp/secret" not in serialized_unsafe_import
assert any(
    fact["fact_kind"] == "UNKNOWN" and fact["target"] == "DynamicImport"
    for fact in unsafe_literal_import[0]["facts"]
)

unresolved_literal_dynamic_import = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "missing_dynamic_import.py",
        "content_hash": "sha256:" + "6" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
import importlib

def load():
    return importlib.import_module("plugins.safe")
""",
    }
)
unresolved_literal_dynamic_import_facts = unresolved_literal_dynamic_import[0]["facts"]
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "DynamicImport"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    for fact in unresolved_literal_dynamic_import_facts
)
assert not any(
    fact["fact_kind"] == "RESOLVED_IMPORT" and fact["target"] == "plugins.safe"
    for fact in unresolved_literal_dynamic_import_facts
)

ambiguous_literal_dynamic_import = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "ambiguous_dynamic_import.py",
        "content_hash": "sha256:" + "6" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": [
            "ambiguous_dynamic_import.py",
            "src/plugins/safe.py",
            "alt/plugins/safe.py",
        ],
        "source_roots": ["src", "alt"],
        "text": """
import importlib

def load():
    return importlib.import_module("plugins.safe")
""",
    }
)
ambiguous_literal_dynamic_import_facts = ambiguous_literal_dynamic_import[0]["facts"]
assert has_unknown_for_subject(
    ambiguous_literal_dynamic_import_facts,
    "load",
    "DynamicImport",
    "python_import_resolution",
)
assert not any(
    fact["fact_kind"] == "RESOLVED_IMPORT"
    and fact["target"] == "plugins.safe"
    and "python_anchor_kind=dynamic_import_literal" in fact["assumptions"]
    for fact in ambiguous_literal_dynamic_import_facts
)
assert_no_fact_source_payloads(ambiguous_literal_dynamic_import_facts)

dynamic_import_boundary = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "dynamic_import_boundary.py",
        "content_hash": "sha256:" + "4" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": ["dynamic_import_boundary.py", "plugins/safe.py"],
        "text": """
import importlib
import sys

def load(name, extra_path):
    sys.path.insert(0, extra_path)
    safe = importlib.import_module("plugins.safe")
    importlib.import_module("../secret")
    importlib.import_module(name)
    handler = getattr(safe, "handle")
    return handler
""",
    }
)
dynamic_import_boundary_facts = dynamic_import_boundary[0]["facts"]
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "RuntimeDependencyInjection"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    for fact in dynamic_import_boundary_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_IMPORT"
    and fact["target"] == "plugins.safe"
    and "python_anchor_kind=dynamic_import_literal" in fact["assumptions"]
    for fact in dynamic_import_boundary_facts
)
assert (
    sum(
        1
        for fact in dynamic_import_boundary_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "DynamicImport"
        and "affected_claim=python_import_resolution" in fact["assumptions"]
    )
    >= 2
)
assert not any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "FrameworkMagic"
    and "affected_claim=python_call_target" in fact["assumptions"]
    for fact in dynamic_import_boundary_facts
)
assert "../secret" not in json.dumps(dynamic_import_boundary)

# Sound constant propagation: a single-static local string constant used as the
# importlib.import_module target resolves exactly like writing the literal, while
# a reassigned name stays a typed DynamicImport UNKNOWN.
const_import_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "const_import.py",
        "content_hash": "sha256:" + "4" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": ["const_import.py", "plugins/safe.py"],
        "text": """
import importlib

def load_constant():
    module_name = "plugins.safe"
    return importlib.import_module(module_name)

def load_reassigned(flag):
    module_name = "plugins.safe"
    if flag:
        module_name = "plugins.other"
    return importlib.import_module(module_name)
""",
    }
)
const_import_facts = const_import_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "RESOLVED_IMPORT"
    and fact["target"] == "plugins.safe"
    and "python_anchor_kind=dynamic_import_literal" in fact["assumptions"]
    for fact in const_import_facts
)
# The reassigned name is ambiguous, so it remains a typed DynamicImport UNKNOWN.
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "DynamicImport"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    for fact in const_import_facts
)
assert "plugins.other" not in json.dumps(const_import_messages)

# The __import__ builtin is statically determinable exactly like
# importlib.import_module: a literal or single-static-constant absolute name
# resolves to the repo-local module, while a relative (nonzero level),
# reassigned, or parameter-bound target stays a typed DynamicImport UNKNOWN.
import_builtin_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "import_builtin.py",
        "content_hash": "sha256:" + "4" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": ["import_builtin.py", "plugins/safe.py"],
        "text": """
def load_literal():
    return __import__("plugins.safe")

def load_constant():
    module_name = "plugins.safe"
    return __import__(module_name)

def load_relative():
    return __import__("plugins.safe", globals(), locals(), [], 1)

def load_reassigned(flag):
    module_name = "plugins.safe"
    if flag:
        module_name = "plugins.other"
    return __import__(module_name)

def load_parameter(name):
    return __import__(name)
""",
    }
)
import_builtin_facts = import_builtin_messages[0]["facts"]
# Literal and single-static-constant targets resolve to the repo-local module.
assert (
    sum(
        1
        for fact in import_builtin_facts
        if fact["fact_kind"] == "RESOLVED_IMPORT"
        and fact["target"] == "plugins.safe"
        and "python_anchor_kind=dynamic_import_literal" in fact["assumptions"]
    )
    == 2
)
# Relative (nonzero level), reassigned, and parameter-bound targets abstain.
assert (
    sum(
        1
        for fact in import_builtin_facts
        if fact["fact_kind"] == "UNKNOWN"
        and fact["target"] == "DynamicImport"
        and "affected_claim=python_import_resolution" in fact["assumptions"]
    )
    == 3
)
# The relative call must not falsely resolve even though "plugins.safe" is a
# repo-local module, and no source text may leak into the facts.
assert not any(
    fact["fact_kind"] == "RESOLVED_IMPORT" and fact["target"] == "plugins.other"
    for fact in import_builtin_facts
)
assert "plugins.other" not in json.dumps(import_builtin_messages)
assert_no_fact_source_payloads(import_builtin_facts)

config_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "pyproject.toml",
        "content_hash": "sha256:" + "5" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
[project]
name = "demo-api"

[tool.pytest.ini_options]
testpaths = ["tests", "../secret"]
pythonpath = ["src", "/tmp/secret"]

[tool.pyright]
include = ["src", "tests"]
extraPaths = ["src/lib", "C:/secret"]

[tool.pyrefly]
project_includes = ["src"]
""",
    }
)
assert len(config_messages) == 1
assert config_messages[0]["mode"] == "parse_project_config"
assert config_messages[0]["path"] == "pyproject.toml"
if sys.version_info >= (3, 11):
    assert config_messages[0]["config"]["project_name"] == "demo-api"
    assert config_messages[0]["config"]["source_roots"] == ["src", "src/lib", "tests"]
    assert config_messages[0]["config"]["tool_sections"] == ["pyrefly", "pyright", "pytest"]
    assert config_messages[0]["unknowns"] == []
else:
    assert config_messages[0]["config"]["source_roots"] == []
    assert config_messages[0]["unknowns"] == [
        {"reason": "MissingDependency", "affected_claim": "python_project_config"}
    ]
serialized_config = json.dumps(config_messages)
assert "../secret" not in serialized_config
assert "/tmp/secret" not in serialized_config
assert "C:/secret" not in serialized_config

bad_config_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "pyproject.toml",
        "content_hash": "sha256:" + "5" * 64,
        "repository_revision": "UNKNOWN",
        "text": "[project\nname = 'broken'\n",
    }
)
if sys.version_info >= (3, 11):
    assert bad_config_messages[0]["unknowns"] == [
        {"reason": "MissingProjectConfig", "affected_claim": "python_project_config"}
    ]
else:
    assert bad_config_messages[0]["unknowns"] == [
        {"reason": "MissingDependency", "affected_claim": "python_project_config"}
    ]
assert "[project" not in json.dumps(bad_config_messages)

# setup.cfg (INI) project config is parsed with the standard-library
# configparser, so it works on every supported Python version (no tomllib
# dependency). Only sanitized name and repo-relative source roots are extracted;
# unsafe paths are dropped.
setup_cfg_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "setup.cfg",
        "content_hash": "sha256:" + "7" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
[metadata]
name = demo-setup-cfg

[options.packages.find]
where = src ../secret

[tool:pytest]
testpaths = tests /tmp/secret
pythonpath = src
""",
    }
)
assert len(setup_cfg_messages) == 1
assert setup_cfg_messages[0]["mode"] == "parse_project_config"
assert setup_cfg_messages[0]["path"] == "setup.cfg"
assert setup_cfg_messages[0]["config"]["project_name"] == "demo-setup-cfg"
assert setup_cfg_messages[0]["config"]["source_roots"] == ["src", "tests"]
assert setup_cfg_messages[0]["config"]["tool_sections"] == ["pytest"]
assert setup_cfg_messages[0]["unknowns"] == []
serialized_setup_cfg = json.dumps(setup_cfg_messages)
assert "../secret" not in serialized_setup_cfg
assert "/tmp/secret" not in serialized_setup_cfg

bad_setup_cfg_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "setup.cfg",
        "content_hash": "sha256:" + "7" * 64,
        "repository_revision": "UNKNOWN",
        "text": "[metadata\nname = broken\n",
    }
)
assert bad_setup_cfg_messages[0]["unknowns"] == [
    {"reason": "MissingProjectConfig", "affected_claim": "python_project_config"}
]
assert "broken" not in json.dumps(bad_setup_cfg_messages)

# setup.py project config is parsed with the standard-library `ast` module and is
# NEVER executed. Only complete literal source-root evidence is read: a unique
# string-to-string package_dir mapping and at most one literal positional-or-
# keyword where for find_packages; unsafe paths are dropped.
setup_py_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "setup.py",
        "content_hash": "sha256:" + "8" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from setuptools import find_namespace_packages, find_packages, setup

DYNAMIC_ROOT = compute_root()
find_namespace_packages("standalone-decoy")

setup(
    name="demo-setup-py",
    package_dir={"": "src", "demo": "src/demo", "bad": "../secret"},
    packages=find_packages(where="src"),
    extra=find_namespace_packages("keyword-decoy"),
    dynamic=find_packages(where=DYNAMIC_ROOT),
)
""",
    }
)
assert len(setup_py_messages) == 1
assert setup_py_messages[0]["mode"] == "parse_project_config"
assert setup_py_messages[0]["path"] == "setup.py"
assert setup_py_messages[0]["config"]["project_name"] == "demo-setup-py"
# "src" and "src/demo" are literal and safe; "../secret" is dropped, the
# DYNAMIC_ROOT-driven finder outside packages= is irrelevant, and finders
# outside packages= do not contribute roots.
assert setup_py_messages[0]["config"]["source_roots"] == ["src", "src/demo"]
assert setup_py_messages[0]["config"]["tool_sections"] == []
assert setup_py_messages[0]["unknowns"] == []
serialized_setup_py = json.dumps(setup_py_messages)
assert "../secret" not in serialized_setup_py
assert "compute_root" not in serialized_setup_py

# setup.py context requires an import-bound setuptools call. Same-leaf local
# functions, unrelated qualified helpers, and bindings shadowed after import
# must not contribute project names or source roots.
for unbound_setup_py in [
    """
def setup(**kwargs):
    return kwargs

def find_packages(*args, **kwargs):
    return []

setup(name="local-project", package_dir={"": "local-src"}, packages=find_packages("local-packages"))
""",
    """
import helper

helper.setup(
    name="helper-project",
    package_dir={"": "helper-src"},
    packages=helper.find_packages(where="helper-packages"),
)
""",
    """
from setuptools import find_packages, setup

setup = helper.setup
find_packages = helper.find_packages
setup(
    name="shadowed-project",
    package_dir={"": "shadowed-src"},
    packages=find_packages(where="shadowed-packages"),
)
""",
    """
from setuptools import setup

if False:
    setup(name="dead-project", package_dir={"": "dead-src"})
""",
    """
from setuptools import setup

if flag:
    setup = helper.setup
setup(name="conditional-shadow", package_dir={"": "conditional-src"})
""",
    """
from setuptools import setup

del setup
setup(name="deleted-project", package_dir={"": "deleted-src"})
""",
    """
from setuptools import find_packages

find_packages(where="standalone-decoy")
""",
    """
import setuptools as build_tools

build_tools.setup = helper.setup
build_tools.setup(name="attribute-shadow", package_dir={"": "attribute-src"})
""",
    """
import setuptools as build_tools

del build_tools.setup
build_tools.setup(name="attribute-deleted", package_dir={"": "attribute-deleted-src"})
""",
    """
import setuptools as build_tools

build_tools.find_packages = helper.find_packages
build_tools.setup(
    name="finder-attribute-shadow",
    packages=build_tools.find_packages(where="finder-attribute-src"),
)
""",
    """
import setuptools as build_tools

setattr(build_tools, "setup", helper.setup)
build_tools.setup(name="setattr-shadow", package_dir={"": "setattr-src"})
""",
    """
import builtins
import setuptools as build_tools

builtins.setattr(build_tools, "setup", helper.setup)
build_tools.setup(name="builtins-setattr", package_dir={"": "builtins-setattr-src"})
""",
    """
import setuptools as build_tools

delattr(build_tools, "find_packages")
build_tools.setup(
    name="delattr-finder",
    packages=build_tools.find_packages(where="delattr-finder-src"),
)
""",
    """
import setuptools as build_tools

globals().update({"build_tools": helper})
build_tools.setup(name="globals-update", package_dir={"": "globals-update-src"})
""",
    """
import setuptools as build_tools

globals()["build_tools"] = helper
build_tools.setup(name="globals-subscript", package_dir={"": "globals-subscript-src"})
""",
    """
import setuptools as build_tools

locals().update({"build_tools": helper})
build_tools.setup(name="locals-update", package_dir={"": "locals-update-src"})
""",
    """
import setuptools as build_tools

vars(build_tools)["setup"] = helper.setup
build_tools.setup(name="vars-shadow", package_dir={"": "vars-src"})
""",
    """
import builtins
import setuptools as build_tools

builtins.vars(build_tools)["setup"] = helper.setup
build_tools.setup(name="builtins-vars", package_dir={"": "builtins-vars-src"})
""",
    """
import setuptools as build_tools

build_tools.__dict__["setup"] = helper.setup
build_tools.setup(name="dict-subscript", package_dir={"": "dict-subscript-src"})
""",
    """
import setuptools as build_tools

build_tools.__dict__.update({"find_packages": helper.find_packages})
build_tools.setup(
    name="dict-update-finder",
    packages=build_tools.find_packages(where="dict-update-src"),
)
""",
]:
    unbound_setup_py_messages = run_worker(
        {
            "protocol_version": 1,
            "mode": "parse_project_config",
            "path": "setup.py",
            "content_hash": "sha256:" + "8" * 64,
            "repository_revision": "UNKNOWN",
            "text": unbound_setup_py,
        }
    )
    assert unbound_setup_py_messages[0]["config"] == {
        "project_name": None,
        "source_roots": [],
        "tool_sections": [],
    }
    assert unbound_setup_py_messages[0]["unknowns"] == []

# A unique setuptools call is still incomplete when its argument shape can
# override or hide the relevant project-config value. Those fields contribute
# no source roots and produce a typed config UNKNOWN instead of a silent partial
# success. Each input is valid AST even when Python would reject it at runtime.
for ambiguous_setup_py in [
    """
from setuptools import setup

setup("positional-name", package_dir={"": "positional-forged"})
""",
    """
from setuptools import setup

setup(**dynamic, package_dir={"": "unpack-forged"})
""",
    """
from setuptools import setup

setup(package_dir={"": "first-root"}, package_dir={"": "duplicate-root"})
""",
    """
from setuptools import setup

setup(name=helper())
""",
    """
from setuptools import setup

setup(packages=dynamic)
""",
    """
from setuptools import setup

setup(name="dynamic-key", package_dir={helper(): "dynamic-key-root"})
""",
    """
from setuptools import setup

setup(name="dict-unpack", package_dir={**mapping, "": "dict-unpack-root"})
""",
    """
from setuptools import setup

setup(name="duplicate-key", package_dir={"": "first-root", "": "duplicate-key-root"})
""",
    """
from setuptools import setup

setup(name="dynamic-value", package_dir={"": helper()})
""",
    """
from setuptools import find_packages, setup

setup(name="positional-where", packages=find_packages("src", where="positional-where-root"))
""",
    """
from setuptools import find_packages, setup

setup(name="finder-unpack", packages=find_packages(where="finder-unpack-root", **dynamic))
""",
    """
from setuptools import find_packages, setup

setup(name="duplicate-where", packages=find_packages(where="first-root", where="duplicate-where-root"))
""",
    """
from setuptools import find_packages, setup

setup(name="dynamic-where", packages=find_packages(where=helper()))
""",
    """
from setuptools import setup

setup(name="lookalike-finder", packages=helper.find_packages(where="lookalike-root"))
""",
    """
from setuptools import setup

raise RuntimeError("setup is unreachable")
setup(name="dead-config", package_dir={"": "dead-config-root"})
""",
]:
    ambiguous_setup_py_messages = run_worker(
        {
            "protocol_version": 1,
            "mode": "parse_project_config",
            "path": "setup.py",
            "content_hash": "sha256:" + "8" * 64,
            "repository_revision": "UNKNOWN",
            "text": ambiguous_setup_py,
        }
    )
    assert ambiguous_setup_py_messages[0]["config"]["source_roots"] == []
    assert ambiguous_setup_py_messages[0]["unknowns"] == [
        {"reason": "MissingProjectConfig", "affected_claim": "python_project_config"}
    ]

empty_setup_py_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "setup.py",
        "content_hash": "sha256:" + "8" * 64,
        "repository_revision": "UNKNOWN",
        "text": "from setuptools import setup\nsetup()\n",
    }
)
assert empty_setup_py_messages[0]["config"] == {
    "project_name": None,
    "source_roots": [],
    "tool_sections": [],
}
assert empty_setup_py_messages[0]["unknowns"] == []

# Direct aliases and qualified module aliases retain precise setuptools
# support, including the package finder nested in the setup call.
aliased_setup_py_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "setup.py",
        "content_hash": "sha256:" + "8" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from setuptools import setup as configure
import setuptools as build_tools

configure(
    name="aliased-project",
    package_dir={"": "aliased-src"},
    packages=build_tools.find_namespace_packages(where="aliased-packages"),
)
""",
    }
)
assert aliased_setup_py_messages[0]["config"] == {
    "project_name": "aliased-project",
    "source_roots": ["aliased-packages", "aliased-src"],
    "tool_sections": [],
}
assert aliased_setup_py_messages[0]["unknowns"] == []

# Multiple independently authoritative setup calls must not be merged into one
# synthetic project config. The worker fails closed with a typed conflict.
conflicting_setup_py_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "setup.py",
        "content_hash": "sha256:" + "8" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from setuptools import setup

setup(name="first-project", package_dir={"": "first-src"})
setup(name="second-project", package_dir={"": "second-src"})
""",
    }
)
assert conflicting_setup_py_messages[0]["config"] == {
    "project_name": None,
    "source_roots": [],
    "tool_sections": [],
}
assert conflicting_setup_py_messages[0]["unknowns"] == [
    {"reason": "ConflictingFacts", "affected_claim": "python_project_config"}
]

# Binding state is scanned once in source order. A large batch of same-leaf
# setup candidates before the authoritative import stays unbound, the final
# imported call is the sole authority, and the pure helper visits each module
# statement exactly once without a timing-sensitive assertion.
pre_import_setup_decoys = "setup()\n" * 2048
linear_setup_py_source = (
    pre_import_setup_decoys
    + "from setuptools import setup\n"
    + "setup(name='linear-project', package_dir={'': 'linear-src'})\n"
)
worker_namespace = runpy.run_path(str(WORKER))
linear_setup_py_tree = ast.parse(linear_setup_py_source)
(
    linear_setup_calls,
    scanned_setup_statements,
    terminated_before_setup,
) = worker_namespace["scan_authoritative_setup_py_calls"](linear_setup_py_tree)
assert scanned_setup_statements == len(linear_setup_py_tree.body)
assert len(linear_setup_calls) == 1
assert not terminated_before_setup
linear_setup_py_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "setup.py",
        "content_hash": "sha256:" + "8" * 64,
        "repository_revision": "UNKNOWN",
        "text": linear_setup_py_source,
    }
)
assert linear_setup_py_messages[0]["config"] == {
    "project_name": "linear-project",
    "source_roots": ["linear-src"],
    "tool_sections": [],
}
assert linear_setup_py_messages[0]["unknowns"] == []

# A setup.py that does not parse yields a typed MissingProjectConfig UNKNOWN
# rather than any guessed context, and its source never leaks.
bad_setup_py_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "setup.py",
        "content_hash": "sha256:" + "8" * 64,
        "repository_revision": "UNKNOWN",
        "text": "setup(name=\nbroken_setup_py(",
    }
)
assert bad_setup_py_messages[0]["unknowns"] == [
    {"reason": "MissingProjectConfig", "affected_claim": "python_project_config"}
]
assert "broken_setup_py" not in json.dumps(bad_setup_py_messages)

parse_context_hash = "sha256:" + "6" * 64
parse_context_payload = {
    "protocol_version": 1,
    "mode": "parse_document",
    "path": "src/acme/api.py",
    "content_hash": parse_context_hash,
    "repository_revision": "UNKNOWN",
    "module_paths": [
        "src/acme/api.py",
        "src/acme/__init__.py",
        "src/acme/services/__init__.py",
        "src/acme/services/users.py",
    ],
    "source_roots": [],
    "text": """
from acme.services import users
from .services import users as relative_users
from acme.missing import value
""",
}
parse_context_messages = run_worker(parse_context_payload)
parse_context_reordered = dict(parse_context_payload)
parse_context_reordered["module_paths"] = list(reversed(parse_context_payload["module_paths"]))
assert parse_context_messages == run_worker(parse_context_reordered)
parse_context_facts = parse_context_messages[0]["facts"]
repo_local_import_facts = [
        fact
        for fact in parse_context_facts
        if fact["fact_kind"] == "RESOLVED_IMPORT"
        and fact["target"] == "acme.services.users"
        and "python_anchor_kind=repo_local_import_binding" in fact["assumptions"]
]
assert len(repo_local_import_facts) == 2
for fact in repo_local_import_facts:
    assert fact["certainty"] == "DATAFLOW_DERIVED"
    assert fact["origin"]["method"] == "cpython_ast"
    assert fact["evidence"]["path"] == "src/acme/api.py"
    assert fact["evidence"]["content_hash"] == parse_context_hash
    assert "provider_resolved=false" in fact["assumptions"]
    assert "derived_from=repo_local_python_import_graph" in fact["assumptions"]
assert not any(
    fact["fact_kind"] == "RESOLVED_IMPORT" and fact["target"] == "acme.missing.value"
    for fact in parse_context_facts
)
unresolved_import_facts = [
    fact
    for fact in parse_context_facts
    if fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "UnresolvedImport"
    and "reason_code=UnresolvedImport" in fact["assumptions"]
    and "affected_claim=python_import_resolution" in fact["assumptions"]
]
assert len(unresolved_import_facts) == 1
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "UnresolvedImport"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    for fact in parse_context_facts
)
serialized_parse_context = json.dumps(parse_context_messages)
assert "from acme.services" not in serialized_parse_context
assert "src/acme/services/users.py" not in serialized_parse_context

symbol_context_hash = "sha256:" + "4" * 64
symbol_context_payload = {
    "protocol_version": 1,
    "mode": "parse_document",
    "path": "src/acme/api.py",
    "content_hash": symbol_context_hash,
    "repository_revision": "UNKNOWN",
    "module_paths": [
        "src/acme/api.py",
        "src/acme/__init__.py",
        "src/acme/models.py",
    ],
    "module_files": [
        {
            "path": "src/acme/__init__.py",
            "text": "from .models import User as PublicUser\n",
        },
        {
            "path": "src/acme/models.py",
            "text": "__all__ = ['User']\nclass User: pass\ndef make_user(): pass\n",
        },
        {
            "path": "src/acme/api.py",
            "text": "",
        },
    ],
    "source_roots": [],
    "text": """
from acme.models import User, make_user
from acme import PublicUser
from acme.models import *
""",
}
symbol_context_facts = run_worker(symbol_context_payload)[0]["facts"]
assert any(
    fact["fact_kind"] == "TYPE"
    and fact["target"] == "acme.models.User"
    and fact["certainty"] == "DATAFLOW_DERIVED"
    and "python_anchor_kind=repo_local_import_symbol" in fact["assumptions"]
    and "derived_from=repo_local_python_import_graph" in fact["assumptions"]
    for fact in symbol_context_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "acme.models.make_user"
    and fact["certainty"] == "DATAFLOW_DERIVED"
    for fact in symbol_context_facts
)
assert not any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "UnresolvedImport"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    for fact in symbol_context_facts
)
serialized_symbol_context = json.dumps(symbol_context_facts)
assert "class User" not in serialized_symbol_context
assert "def make_user" not in serialized_symbol_context

unsafe_star_facts = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "src/acme/api.py",
        "content_hash": "sha256:" + "3" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": ["src/acme/api.py", "src/acme/models.py"],
        "module_files": [
            {"path": "src/acme/models.py", "text": "class User: pass\n"},
            {"path": "src/acme/api.py", "text": ""},
        ],
        "source_roots": [],
        "text": "from acme.models import *\n",
    }
)[0]["facts"]
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "UnresolvedImport"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    for fact in unsafe_star_facts
)

conftest_parse_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "tests/sub/test_api.py",
        "content_hash": "sha256:" + "9" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": ["tests/conftest.py", "tests/sub/test_api.py"],
        "source_roots": [],
        "conftest_files": [
            {
                "path": "tests/conftest.py",
                "text": """
import pytest as pt

@pt.fixture
def client():
    return object()
""",
            }
        ],
        "text": """
def test_users(client, missing_fixture):
    assert client is not None
""",
    }
)
conftest_parse_facts = conftest_parse_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.test"
    and "python_anchor_kind=pytest_test_function" in fact["assumptions"]
    for fact in conftest_parse_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture.client"
    and "python_anchor_kind=pytest_conftest_fixture_edge" in fact["assumptions"]
    for fact in conftest_parse_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "PytestFixtureInjection"
    and "affected_claim=pytest_fixture_binding" in fact["assumptions"]
    for fact in conftest_parse_facts
)
serialized_conftest_parse = json.dumps(conftest_parse_messages)
assert "tests/conftest.py" not in serialized_conftest_parse
assert "return object" not in serialized_conftest_parse
assert "missing_fixture" not in serialized_conftest_parse

fixture_boundary_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "tests/sub/test_fixture_boundaries.py",
        "content_hash": "sha256:" + "b" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": [
            "conftest.py",
            "tests/conftest.py",
            "tests/sub/test_fixture_boundaries.py",
        ],
        "source_roots": [],
        "conftest_files": [
            {
                "path": "conftest.py",
                "text": """
import pytest

@pytest.fixture
def client():
    return object()
""",
            },
            {
                "path": "tests/conftest.py",
                "text": """
import pytest

@pytest.fixture
def client():
    return object()
""",
            },
        ],
        "text": """
def test_fixture_boundaries(client, tmp_path, capsys, django_db):
    assert tmp_path
""",
    }
)
fixture_boundary_facts = fixture_boundary_messages[0]["facts"]
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture.client"
    and "python_anchor_kind=pytest_conftest_fixture_edge" in fact["assumptions"]
    for fact in fixture_boundary_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "ConflictingFacts"
    and "affected_claim=pytest_fixture_binding" in fact["assumptions"]
    for fact in fixture_boundary_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.builtin_fixture.tmp_path"
    and "python_anchor_kind=pytest_builtin_fixture_context" in fact["assumptions"]
    for fact in fixture_boundary_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.builtin_fixture.capsys"
    and "python_anchor_kind=pytest_builtin_fixture_context" in fact["assumptions"]
    for fact in fixture_boundary_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "PytestFixtureInjection"
    and "affected_claim=pytest_fixture_binding" in fact["assumptions"]
    for fact in fixture_boundary_facts
)
serialized_fixture_boundaries = json.dumps(fixture_boundary_messages)
assert "tests/conftest.py" not in serialized_fixture_boundaries
assert "return object" not in serialized_fixture_boundaries
assert "django_db" not in serialized_fixture_boundaries

fixture_name_alias_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "tests/test_fixture_alias_name.py",
        "content_hash": "sha256:" + "c" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": ["tests/test_fixture_alias_name.py"],
        "source_roots": [],
        "conftest_files": [],
        "text": """
import pytest

fixture_name = "dynamic_client"
fixture_alias = pytest.fixture

@pytest.fixture(name="api_client")
def _api_client():
    return object()

@pytest.fixture(name=fixture_name)
def dynamic_client():
    return object()

@fixture_alias(name="settings")
def _settings():
    return object()

@pytest.fixture(name="bad/client")
def unsafe_client():
    return object()

def test_fixture_aliases(api_client, settings, _api_client, dynamic_client, unsafe_client):
    assert api_client
""",
    }
)
fixture_name_alias_facts = fixture_name_alias_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture.api_client"
    and "python_anchor_kind=pytest_fixture_edge" in fact["assumptions"]
    for fact in fixture_name_alias_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.fixture.settings"
    and "python_anchor_kind=pytest_fixture_edge" in fact["assumptions"]
    for fact in fixture_name_alias_facts
)
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"]
    in {
        "pytest.fixture._api_client",
        "pytest.fixture._settings",
        "pytest.fixture.dynamic_client",
        "pytest.fixture.unsafe_client",
    }
    and "python_anchor_kind=pytest_fixture_edge" in fact["assumptions"]
    for fact in fixture_name_alias_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "PytestFixtureInjection"
    and "affected_claim=pytest_fixture_binding" in fact["assumptions"]
    for fact in fixture_name_alias_facts
)
serialized_fixture_name_aliases = json.dumps(fixture_name_alias_messages)
assert "name=fixture_name" not in serialized_fixture_name_aliases
assert "bad/client" not in serialized_fixture_name_aliases
assert "return object" not in serialized_fixture_name_aliases

ambiguous_context_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "src/pkg/api.py",
        "content_hash": "sha256:" + "8" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": ["src/pkg/api.py", "src/pkg/util.py", "alt/pkg/util.py"],
        "source_roots": ["src", "alt"],
        "text": "from pkg import util\n",
    }
)
ambiguous_context_facts = ambiguous_context_messages[0]["facts"]
assert not any(
    fact["fact_kind"] == "RESOLVED_IMPORT"
    and fact["target"] == "pkg.util"
    and "python_anchor_kind=repo_local_import_binding" in fact["assumptions"]
    for fact in ambiguous_context_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "UnresolvedImport"
    and "reason_code=UnresolvedImport" in fact["assumptions"]
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    for fact in ambiguous_context_facts
)

bad_parse_context_payload = {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "app.py",
        "content_hash": "sha256:" + "7" * 64,
        "repository_revision": "UNKNOWN",
        "module_paths": ["../secret.py"],
        "text": "import secret\n",
}
bad_parse_context = subprocess.run(
    [sys.executable, str(WORKER)],
    input=json.dumps(bad_parse_context_payload) + "\n",
    text=True,
    capture_output=True,
    check=False,
)
assert bad_parse_context.returncode == 2
assert bad_parse_context.stdout == ""
assert "secret" not in bad_parse_context.stderr

bad_conftest_context_payload = {
    "protocol_version": 1,
    "mode": "parse_document",
    "path": "app.py",
    "content_hash": "sha256:" + "a" * 64,
    "repository_revision": "UNKNOWN",
    "conftest_files": [{"path": "../conftest.py", "text": "def secret(): pass\n"}],
    "text": "def test_secret(secret):\n    pass\n",
}
bad_conftest_context = subprocess.run(
    [sys.executable, str(WORKER)],
    input=json.dumps(bad_conftest_context_payload) + "\n",
    text=True,
    capture_output=True,
    check=False,
)
assert bad_conftest_context.returncode == 2
assert bad_conftest_context.stdout == ""
assert "secret" not in bad_conftest_context.stderr

with tempfile.TemporaryDirectory() as root:
    Path(root, "pyproject.toml").write_text(
        """
[tool.pyright]
include = ["src", "../secret", "/tmp/secret"]
extraPaths = ["src/lib", "C:/secret"]
""",
        encoding="utf-8",
    )
    Path(root, "acme/services").mkdir(parents=True)
    Path(root, "acme/__init__.py").write_text("", encoding="utf-8")
    Path(root, "acme/services/__init__.py").write_text("", encoding="utf-8")
    Path(root, "acme/services/users.py").write_text("def list_users():\n    return []\n", encoding="utf-8")
    Path(root, "acme/api.py").write_text(
        """
from acme.services import users
from .services import users as relative_users
import acme.services.users as user_module
from acme.missing import value
""",
        encoding="utf-8",
    )
    request = valid_request(root)
    request["changed_files"] = [
        "acme/api.py",
        "acme/services/users.py",
        "acme/__init__.py",
        "acme/services/__init__.py",
    ]
    messages = run_worker(request)
    assert_end_of_stream(messages)
    facts = [message for message in messages if message.get("message_type") == "fact"]
    repo_local_imports = [
        fact
        for fact in facts
        if fact.get("fact_kind") == "RESOLVED_IMPORT"
        and fact.get("target") == "acme.services.users"
        and "python_anchor_kind=repo_local_import_binding" in fact.get("assumptions", [])
    ]
    assert len(repo_local_imports) == 3
    assert any(
        fact.get("fact_kind") == "UNKNOWN"
        and fact.get("target") == "UnresolvedImport"
        and "affected_claim=python_import_resolution" in fact.get("assumptions", [])
        for fact in facts
    )
    serialized_module_graph = json.dumps(messages)
    assert "../secret" not in serialized_module_graph
    assert "/tmp/secret" not in serialized_module_graph
    assert "C:/secret" not in serialized_module_graph
    assert root not in serialized_module_graph
    assert "from acme.services" not in serialized_module_graph

if sys.version_info >= (3, 11):
    with tempfile.TemporaryDirectory() as root:
        Path(root, "pyproject.toml").write_text(
            """
[tool.pyright]
include = ["src", "alt"]
""",
            encoding="utf-8",
        )
        Path(root, "src/pkg").mkdir(parents=True)
        Path(root, "alt/pkg").mkdir(parents=True)
        Path(root, "src/pkg/util.py").write_text("VALUE = 1\n", encoding="utf-8")
        Path(root, "alt/pkg/util.py").write_text("VALUE = 2\n", encoding="utf-8")
        Path(root, "src/pkg/api.py").write_text("from pkg import util\n", encoding="utf-8")
        request = valid_request(root)
        request["changed_files"] = ["src/pkg/api.py", "src/pkg/util.py", "alt/pkg/util.py"]
        messages = run_worker(request)
        facts = [message for message in messages if message.get("message_type") == "fact"]
        assert_end_of_stream(messages)
        assert not any(
            fact.get("fact_kind") == "RESOLVED_IMPORT"
            and fact.get("target") == "pkg.util"
            and "python_anchor_kind=repo_local_import_binding" in fact.get("assumptions", [])
            for fact in facts
        )
        assert any(
            fact.get("fact_kind") == "UNKNOWN"
            and fact.get("target") == "UnresolvedImport"
            and "affected_claim=python_import_resolution" in fact.get("assumptions", [])
            for fact in facts
        )

with tempfile.TemporaryDirectory() as root:
    Path(root, "tests/sub").mkdir(parents=True)
    Path(root, "tests/conftest.py").write_text(
        """
import pytest as pt

@pt.fixture
def client():
    return object()
""",
        encoding="utf-8",
    )
    Path(root, "tests/sub/test_api.py").write_text(
        """
def test_users(client, missing_fixture):
    assert client is not None
""",
        encoding="utf-8",
    )
    request = valid_request(root)
    request["changed_files"] = ["tests/sub/test_api.py", "tests/conftest.py"]
    messages = run_worker(request)
    assert_end_of_stream(messages)
    facts = [message for message in messages if message.get("message_type") == "fact"]
    assert any(
        fact.get("fact_kind") == "SYMBOL"
        and fact.get("target") == "pytest.fixture.client"
        and "python_anchor_kind=pytest_conftest_fixture_edge" in fact.get("assumptions", [])
        for fact in facts
    )
    assert any(
        fact.get("fact_kind") == "UNKNOWN"
        and fact.get("target") == "PytestFixtureInjection"
        and "affected_claim=pytest_fixture_binding" in fact.get("assumptions", [])
        for fact in facts
    )
    serialized_conftest = json.dumps(messages)
    assert root not in serialized_conftest
    assert "return object" not in serialized_conftest
    assert "missing_fixture" not in serialized_conftest

with tempfile.TemporaryDirectory() as root:
    Path(root, "app.py").write_text(
        """
from fastapi import APIRouter
from fastapi import Depends
from pydantic import BaseModel
router = APIRouter()

class UserOut(BaseModel):
    id: int

def get_user():
    return object()

@router.post("/users", response_model=UserOut)
def create_user(current_user=Depends(dependency=get_user)):
    return {}
""",
        encoding="utf-8",
    )
    messages = run_worker(valid_request(root))
    assert_end_of_stream(messages)
    assert any(message.get("fact_kind") == "FRAMEWORK_ROLE" for message in messages)
    assert any(
        message.get("fact_kind") == "RESOLVED_IMPORT" and message.get("target") == "fastapi.APIRouter"
        for message in messages
    )
    assert any(
        message.get("fact_kind") == "SYMBOL" and message.get("target") == "fastapi.APIRouter.post"
        for message in messages
    )
    assert any(
        message.get("fact_kind") == "TYPE"
        and message.get("target") == "fastapi.response_model.UserOut"
        and "python_anchor_kind=fastapi_response_model" in message.get("assumptions", [])
        for message in messages
    )
    assert any(
        message.get("fact_kind") == "SYMBOL"
        and message.get("target") == "fastapi.dependency.get_user"
        and "python_anchor_kind=fastapi_dependency_target" in message.get("assumptions", [])
        for message in messages
    )
    serialized = json.dumps(messages)
    assert root not in serialized
    assert "@router.post" not in serialized
    assert "response_model=" not in serialized
    assert "Depends(" not in serialized

with tempfile.TemporaryDirectory() as root:
    Path(root, "app.py").write_text(
        """
from fastapi import APIRouter, FastAPI

app = FastAPI()
router = APIRouter()
app.include_router(router, prefix="/api/v1")
""",
        encoding="utf-8",
    )
    messages = run_worker(valid_request(root))
    assert_end_of_stream(messages)
    facts = [message for message in messages if message.get("message_type") == "fact"]
    include_fact = next(
        fact
        for fact in facts
        if fact.get("fact_kind") == "RESOLVED_CALL"
        and fact.get("target") == "fastapi.FastAPI.include_router"
    )
    assert "python_anchor_kind=fastapi_include_router" in include_fact.get("assumptions", [])
    assert "fact_scope=context_only" in include_fact.get("assumptions", [])
    assert "router_binding=local" in include_fact.get("assumptions", [])
    assert "router_local_name=router" in include_fact.get("assumptions", [])
    assert "route_prefix_shape=/api/v1" in include_fact.get("assumptions", [])
    assert "prefix_unknown=false" in include_fact.get("assumptions", [])
    assert_no_fact_source_payloads(facts)

    Path(root, "routes.py").write_text(
        """
from fastapi import APIRouter

router = APIRouter()
""",
        encoding="utf-8",
    )
    Path(root, "app.py").write_text(
        """
from fastapi import FastAPI
from routes import router

app = FastAPI()
app.include_router(router, prefix="/api")
""",
        encoding="utf-8",
    )
    request = valid_request(root)
    request["changed_files"] = ["app.py", "routes.py"]
    messages = run_worker(request)
    assert_end_of_stream(messages)
    facts = [message for message in messages if message.get("message_type") == "fact"]
    imported_include_fact = next(
        fact
        for fact in facts
        if fact.get("fact_kind") == "RESOLVED_CALL"
        and fact.get("target") == "fastapi.FastAPI.include_router"
    )
    assert "router_binding=repo_local_import" in imported_include_fact.get("assumptions", [])
    assert "router_target=routes.router" in imported_include_fact.get("assumptions", [])
    assert "route_prefix_shape=/api" in imported_include_fact.get("assumptions", [])
    assert_no_fact_source_payloads(facts)

with tempfile.TemporaryDirectory() as root:
    Path(root, "app.py").write_text(
        """
from fastapi import APIRouter, FastAPI

app = FastAPI()
router = APIRouter()
prefix = "/api"
app.include_router(router, prefix=prefix)
""",
        encoding="utf-8",
    )
    messages = run_worker(valid_request(root))
    assert_end_of_stream(messages)
    facts = [message for message in messages if message.get("message_type") == "fact"]
    assert not any(
        fact.get("fact_kind") == "RESOLVED_CALL"
        and fact.get("target") == "fastapi.FastAPI.include_router"
        for fact in facts
    )
    assert any(
        fact.get("fact_kind") == "UNKNOWN"
        and fact.get("target") == "FrameworkMagic"
        and "affected_claim=fastapi_router_prefix" in fact.get("assumptions", [])
        for fact in facts
    )
    assert_no_fact_source_payloads(facts)

with tempfile.TemporaryDirectory() as root:
    Path(root, "app.py").write_text(
        """
from fastapi import FastAPI
from external_routes import router

app = FastAPI()
app.include_router(router, prefix="/api")
""",
        encoding="utf-8",
    )
    messages = run_worker(valid_request(root))
    assert_end_of_stream(messages)
    facts = [message for message in messages if message.get("message_type") == "fact"]
    assert not any(
        fact.get("fact_kind") == "RESOLVED_CALL"
        and fact.get("target") == "fastapi.FastAPI.include_router"
        for fact in facts
    )
    assert any(
        fact.get("fact_kind") == "UNKNOWN"
        and fact.get("target") == "UnresolvedImport"
        and "affected_claim=fastapi_router_binding" in fact.get("assumptions", [])
        for fact in facts
    )
    assert_no_fact_source_payloads(facts)

with tempfile.TemporaryDirectory() as root:
    Path(root, "b.py").write_text("def b():\n    return 2\n", encoding="utf-8")
    Path(root, "a.py").write_text("def a():\n    return 1\n", encoding="utf-8")
    first = valid_request(root)
    first["changed_files"] = ["b.py", "a.py"]
    second = valid_request(root)
    second["changed_files"] = ["a.py", "b.py"]
    assert run_worker(first) == run_worker(second)

if hasattr(os, "symlink"):
    with tempfile.TemporaryDirectory() as root, tempfile.TemporaryDirectory() as outside:
        Path(outside, "outside.py").write_text(
            """
from fastapi import APIRouter
router = APIRouter()

@router.get("/outside")
def outside_route():
    return {}
""",
            encoding="utf-8",
        )
        try:
            os.symlink(Path(outside, "outside.py"), Path(root, "link.py"))
        except OSError:
            pass
        else:
            request = valid_request(root)
            request["changed_files"] = ["link.py"]
            messages = run_worker(request)
            assert_end_of_stream(messages)
            assert not any(message.get("fact_kind") == "FRAMEWORK_ROLE" for message in messages)
            assert outside not in json.dumps(messages)

for changed_files in [
    ["/tmp/secret.py"],
    ["../secret.py"],
    ["src/../secret.py"],
    ["./app.py"],
    ["src\\app.py"],
    ["file:///tmp/secret.py"],
    ["C:tmp/source.py"],
    ["app.py", "app.py"],
]:
    with tempfile.TemporaryDirectory() as root:
        request = valid_request(root)
        request["changed_files"] = changed_files
        messages = run_worker(request)
        assert messages[0]["error_code"] == "SEMANTIC_PROTOCOL_VIOLATION"
        assert_end_of_stream(messages)
        assert "/tmp/secret" not in json.dumps(messages)

# A well-known plugin fixture (here pytest-mock's `mocker`) that is not defined
# repo-locally or in conftest resolves to external plugin fixture context via the
# bounded allowlist instead of a `PytestFixtureInjection` UNKNOWN, while an
# unknown parameter still stays UNKNOWN.
plugin_fixture_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "test_plugin_fixtures.py",
        "content_hash": "sha256:" + "0" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
def test_with_plugin_fixtures(mocker, event_loop, unknown_fixture):
    assert mocker is not None
""",
    }
)
plugin_fixture_facts = plugin_fixture_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.plugin_fixture.mocker"
    and "python_anchor_kind=pytest_plugin_fixture_context" in fact["assumptions"]
    for fact in plugin_fixture_facts
)
assert any(
    fact["fact_kind"] == "SYMBOL"
    and fact["target"] == "pytest.plugin_fixture.event_loop"
    and "python_anchor_kind=pytest_plugin_fixture_context" in fact["assumptions"]
    for fact in plugin_fixture_facts
)
plugin_fixture_unknown_names = [
    fact
    for fact in plugin_fixture_facts
    if fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "PytestFixtureInjection"
    and "affected_claim=pytest_fixture_binding" in fact["assumptions"]
]
# `mocker` and `event_loop` are resolved to plugin context; only the genuinely
# unknown parameter remains a fixture-binding UNKNOWN.
assert len(plugin_fixture_unknown_names) == 1
for fact in plugin_fixture_facts:
    if fact["target"] and fact["target"].startswith("pytest.plugin_fixture."):
        assert fact["certainty"] == "STRUCTURAL"

oversized = run_worker("x" * 1_048_577)
assert oversized[0]["error_code"] == "SEMANTIC_PROTOCOL_VIOLATION"
assert_end_of_stream(oversized)

# A deeply chained attribute expression recurses once per link. It must never
# crash the worker (nonzero exit / traceback / truncated stream): the worker
# must always exit 0 and emit a well-formed protocol stream — either a normal
# parse_document response, or a worker_error terminated by end_of_stream when a
# recursion/resource limit is hit. run_worker asserts returncode == 0 and empty
# stderr, so a crash fails here regardless.
deep_chain = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "deep.py",
        "content_hash": "sha256:" + "0" * 64,
        "repository_revision": "UNKNOWN",
        "text": "value = a" + ".b" * 3000 + "\n",
    }
)
assert deep_chain, "worker must emit a protocol stream for a deep chain"
if deep_chain[0].get("mode") == "parse_document":
    assert deep_chain[0]["path"] == "deep.py"
else:
    assert deep_chain[0]["message_type"] == "worker_error"
    assert deep_chain[-1]["message_type"] == "end_of_stream"

# The dispatch catch-all and the aggregate source-size budget are exercised by
# loading the worker as a module so failures can be injected deterministically.
import ast
import importlib.util
import io

_spec = importlib.util.spec_from_file_location("repogrammar_worker_under_test", str(WORKER))
_worker = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_worker)

# The recursion-depth guard stops dotted_name/static_type_name from overflowing
# the stack on an attribute chain deeper than the guard. Build the node directly
# to isolate the helper from ast.parse's own C recursion limit. Reaching these
# assertions without a RecursionError is the invariant; the returned name is
# bounded by the guard depth rather than the (unbounded) chain length.
_deep_node = ast.Name(id="a")
for _ in range(5000):
    _deep_node = ast.Attribute(value=_deep_node, attr="b")
_deep_dotted = _worker.dotted_name(_deep_node)
assert _deep_dotted is None or (
    isinstance(_deep_dotted, str)
    and len(_deep_dotted) <= 3 * (_worker.MAX_NAME_RECURSION_DEPTH + 2)
)
_deep_typed = _worker.static_type_name(_deep_node)
assert _deep_typed is None or isinstance(_deep_typed, str)


def _run_worker_main(module, payload_text):
    stdin = io.TextIOWrapper(io.BytesIO(payload_text.encode("utf-8")))
    stdout = io.StringIO()
    saved_stdin, saved_stdout = sys.stdin, sys.stdout
    sys.stdin, sys.stdout = stdin, stdout
    try:
        code = module.main()
    finally:
        sys.stdin, sys.stdout = saved_stdin, saved_stdout
    lines = [json.loads(line) for line in stdout.getvalue().splitlines() if line.strip()]
    return code, lines


# An unexpected internal failure (e.g. RecursionError) is converted into a typed
# worker_error + end_of_stream, not a truncated stream and nonzero exit.
def _boom(_payload):
    raise RecursionError("synthetic recursion failure")


_worker.dispatch = _boom
_code, _lines = _run_worker_main(_worker, json.dumps(valid_request("/tmp")) + "\n")
assert _code == 0
assert _lines[0]["message_type"] == "worker_error"
assert _lines[0]["error_code"] == "SEMANTIC_WORKER_FAILURE"
assert_end_of_stream(_lines)

# The aggregate source-size budget fails closed with a worker_error rather than
# reading an unbounded amount of source into memory.
_spec_budget = importlib.util.spec_from_file_location("repogrammar_worker_budget", str(WORKER))
_worker_budget = importlib.util.module_from_spec(_spec_budget)
_spec_budget.loader.exec_module(_worker_budget)
_worker_budget.MAX_TOTAL_SOURCE_BYTES = 1
with tempfile.TemporaryDirectory() as _budget_root:
    Path(_budget_root, "app.py").write_text("value = 1\n", encoding="utf-8")
    _budget_code, _budget_lines = _run_worker_main(
        _worker_budget, json.dumps(valid_request(_budget_root)) + "\n"
    )
assert _budget_code == 0
assert _budget_lines[0]["message_type"] == "worker_error"
assert _budget_lines[0]["error_code"] == "SEMANTIC_PROTOCOL_VIOLATION"
assert "aggregate source-size budget" in _budget_lines[0]["message"]
assert_end_of_stream(_budget_lines)


# ---------------------------------------------------------------------------
# Wave E1-Python bounded preview anchors: Django, Flask, stdlib unittest,
# click/typer, and Celery. Each covers the exact-import gate, the shape
# anchors, and the typed UNKNOWN recipes.
# ---------------------------------------------------------------------------


def _preview_parse(text: str):
    messages = run_worker(
        {
            "protocol_version": 1,
            "mode": "parse_document",
            "path": "app.py",
            "content_hash": "sha256:" + "0" * 64,
            "repository_revision": "UNKNOWN",
            "text": text,
        }
    )
    assert len(messages) == 1
    document = messages[0]
    kinds = [unit["kind"] for unit in document["units"]]
    facts = document["facts"]
    return kinds, facts


def _anchor_targets(facts, anchor_kind):
    return {
        fact["target"]
        for fact in facts
        if any(a == f"python_anchor_kind={anchor_kind}" for a in fact["assumptions"])
    }


def _unknown_claims(facts):
    claims = []
    for fact in facts:
        if fact["fact_kind"] != "UNKNOWN":
            continue
        for assumption in fact["assumptions"]:
            if assumption.startswith("affected_claim="):
                claims.append(assumption.split("=", 1)[1])
    return claims


# Django models: exact base + field-count/meta shape anchors, settings UNKNOWN.
django_model_kinds, django_model_facts = _preview_parse(
    "from django.db import models\n"
    "\n"
    "class Author(models.Model):\n"
    "    name = models.CharField(max_length=1)\n"
    "    email = models.EmailField()\n"
    "    class Meta:\n"
    "        ordering = []\n"
)
assert "django_model" in django_model_kinds
assert any(
    fact["fact_kind"] == "TYPE" and fact["target"] == "django.db.models.Model"
    for fact in django_model_facts
)
assert "django.field_count.1_to_3" in _anchor_targets(django_model_facts, "django_model_field")
assert "django.model_meta.present" in _anchor_targets(django_model_facts, "django_model_meta")
assert "python_django_settings_behavior" in _unknown_claims(django_model_facts)

# Django model lookalike via an imported non-django base stays UNKNOWN.
django_lookalike_kinds, django_lookalike_facts = _preview_parse(
    "from myapp import models\n"
    "\n"
    "class Post(models.Model):\n"
    "    title = models.CharField(max_length=1)\n"
)
assert "django_model" not in django_lookalike_kinds
assert "python_django_model_identity" in _unknown_claims(django_lookalike_facts)

# Django test case: exact base + test-method-count shape anchor.
django_test_kinds, django_test_facts = _preview_parse(
    "from django.test import TestCase\n"
    "\n"
    "class AuthorTests(TestCase):\n"
    "    def test_a(self):\n"
    "        pass\n"
    "    def test_b(self):\n"
    "        pass\n"
)
assert "django_test" in django_test_kinds
assert any(
    fact["fact_kind"] == "TYPE" and fact["target"] == "django.test.TestCase"
    for fact in django_test_facts
)
assert "django.test_method_count.1_to_3" in _anchor_targets(
    django_test_facts, "django_test_method"
)

# Django url patterns: literal path() call becomes a unit; string view and
# non-literal route become typed UNKNOWNs.
django_url_kinds, django_url_facts = _preview_parse(
    "from django.urls import path\n"
    "\n"
    "def view():\n"
    "    pass\n"
    "\n"
    "prefix = '/dyn'\n"
    "urlpatterns = [\n"
    "    path('users/', view),\n"
    "    path('home/', 'app.views.home'),\n"
    "    path(prefix, view),\n"
    "]\n"
)
assert django_url_kinds.count("django_url_pattern") == 2
assert "django.urls.path" in _anchor_targets(django_url_facts, "django_url_route")
url_claims = _unknown_claims(django_url_facts)
assert "python_django_string_dispatch" in url_claims
assert "python_django_url_identity" in url_claims

# Flask routes: Flask(__name__) receiver + literal rule; method shortcut and
# non-literal path recipes.
flask_kinds, flask_facts = _preview_parse(
    "from flask import Flask\n"
    "\n"
    "app = Flask(__name__)\n"
    "\n"
    "@app.route('/health')\n"
    "def health():\n"
    "    return {}\n"
    "\n"
    "@app.post('/users')\n"
    "def create():\n"
    "    return {}\n"
    "\n"
    "@app.route(build())\n"
    "def dynamic():\n"
    "    return {}\n"
)
assert flask_kinds.count("flask_route") == 3
assert "flask.route" in _anchor_targets(flask_facts, "flask_route_decorator")
flask_methods = _anchor_targets(flask_facts, "flask_route_method")
assert "flask.http_method.request" in flask_methods
assert "flask.http_method.post" in flask_methods
assert "python_flask_route_identity" in _unknown_claims(flask_facts)

# Flask lookalike without the flask import is not recognized.
flask_lookalike_kinds, _flask_lookalike_facts = _preview_parse(
    "app = Flask(__name__)\n"
    "\n"
    "@app.route('/health')\n"
    "def health():\n"
    "    return {}\n"
)
assert "flask_route" not in flask_lookalike_kinds

# stdlib unittest: test_* methods in a unittest.TestCase subclass, setUp
# fixture shape, and @patch non-blocking recipe.
unittest_kinds, unittest_facts = _preview_parse(
    "import unittest\n"
    "from unittest.mock import patch\n"
    "\n"
    "class CoreTests(unittest.TestCase):\n"
    "    def setUp(self):\n"
    "        self.value = 1\n"
    "    @patch('app.dependency')\n"
    "    def test_one(self, mocked):\n"
    "        self.assertEqual(self.value, 1)\n"
    "    def test_two(self):\n"
    "        self.assertTrue(self.value)\n"
)
assert unittest_kinds.count("unittest_test_method") == 2
assert "setUp" not in [k for k in unittest_kinds if k == "unittest_test_method"]
assert "unittest.TestCase.test" in _anchor_targets(unittest_facts, "unittest_test_method")
assert "unittest.fixture.setup_only" in _anchor_targets(unittest_facts, "unittest_fixture")
assert "python_unittest_patch_target" in _unknown_claims(unittest_facts)

# click command: command decorator + option param-count shape.
click_kinds, click_facts = _preview_parse(
    "import click\n"
    "\n"
    "@click.command()\n"
    "@click.option('--name')\n"
    "def hello(name):\n"
    "    pass\n"
)
assert "click_command" in click_kinds
assert "click.command" in _anchor_targets(click_facts, "click_command_decorator")
assert "cli.param_count.1_to_3" in _anchor_targets(click_facts, "cli_param_count")

# typer command: Typer() receiver binding + command decorator.
typer_kinds, typer_facts = _preview_parse(
    "import typer\n"
    "\n"
    "app = typer.Typer()\n"
    "\n"
    "@app.command()\n"
    "def hello(name: str):\n"
    "    pass\n"
)
assert "typer_command" in typer_kinds
assert "typer.command" in _anchor_targets(typer_facts, "typer_command_decorator")

# Celery task: shared_task decorator + runtime-routing recipe.
celery_kinds, celery_facts = _preview_parse(
    "from celery import shared_task\n"
    "\n"
    "@shared_task\n"
    "def process():\n"
    "    process.delay()\n"
)
assert "celery_task" in celery_kinds
assert "celery.shared_task" in _anchor_targets(celery_facts, "celery_task_decorator")
assert "python_celery_runtime_routing" in _unknown_claims(celery_facts)

# Celery app.task binding resolves to the celery.task support target.
celery_app_kinds, celery_app_facts = _preview_parse(
    "from celery import Celery\n"
    "\n"
    "app = Celery('proj')\n"
    "\n"
    "@app.task\n"
    "def work():\n"
    "    pass\n"
)
assert "celery_task" in celery_app_kinds
assert "celery.task" in _anchor_targets(celery_app_facts, "celery_task_decorator")

# The checked-in worker is a real large-module regression fixture. It must
# analyze itself within the bounded subprocess timeout without truncating its
# source-free metadata response or exceeding the fact-count contract.
self_source = WORKER.read_text()
self_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "src/workers/python/worker.py",
        "content_hash": "sha256:" + hashlib.sha256(self_source.encode()).hexdigest(),
        "repository_revision": "UNKNOWN",
        "text": self_source,
    }
)
assert len(self_messages) == 1
self_response = self_messages[0]
assert self_response["diagnostics"] == []
assert len(self_response["units"]) > 100
assert 1_000 < len(self_response["facts"]) <= 2_000
assert_no_fact_source_payloads(self_response["facts"])
