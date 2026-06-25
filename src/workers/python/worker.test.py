#!/usr/bin/env python3
"""Smoke tests for the dependency-free Python worker."""

from __future__ import annotations

import json
import os
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
from pydantic import BaseModel, ConfigDict, computed_field, field_validator, model_validator
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
    display_name: str

    @field_validator("id")
    @classmethod
    def validate_id(cls, value):
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

class User(Base):
    id: Mapped[int] = mapped_column(primary_key=True)
    accounts = relationship("Account")

class UserRepository:
    def list_users(self, session: Session):
        session.add(User())
        return session.execute("select users")

    def get_user(self, session: Session):
        return session.scalar("select user")

    def stream_users(self, session: Session):
        return session.scalars("select users")

    async def list_accounts(self, db: AsyncSession):
        return await db.execute("select accounts")

    async def get_account(self, db: AsyncSession):
        return await db.scalar("select account")

    async def stream_accounts(self, db: AsyncSession):
        return await db.scalars("select accounts")

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
    fact["fact_kind"] == "RESOLVED_CALL" and fact["target"] == "sqlalchemy.orm.Session.add"
    for fact in parse_facts
)
assert any(
    fact["fact_kind"] == "RESOLVED_CALL" and fact["target"] == "sqlalchemy.orm.Session.execute"
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
    assert fact["certainty"] in {"STRUCTURAL", "UNKNOWN"}
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
assert not any(
    fact["fact_kind"] == "SYMBOL"
    and "python_anchor_kind=fastapi_http_exception_status" in fact["assumptions"]
    for fact in alias_facts
)
assert "@v1.get" not in json.dumps(alias_parse_messages)
assert "response_model=" not in json.dumps(alias_parse_messages)
assert "Depends(make_dependency" not in json.dumps(alias_parse_messages)
assert "HTTPException(" not in json.dumps(alias_parse_messages)

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
assert "decorator_factory(\"secret\")" not in json.dumps(dynamic_messages)
assert "setattr(obj" not in json.dumps(dynamic_messages)
assert "/tmp/secret" not in json.dumps(dynamic_messages)

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

config_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "pyproject.toml",
        "content_hash": "sha256:" + "4" * 64,
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
    assert fact["certainty"] == "STRUCTURAL"
    assert fact["origin"]["method"] == "cpython_ast"
    assert fact["evidence"]["path"] == "src/acme/api.py"
    assert fact["evidence"]["content_hash"] == parse_context_hash
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

oversized = run_worker("x" * 1_048_577)
assert oversized[0]["error_code"] == "SEMANTIC_PROTOCOL_VIOLATION"
assert_end_of_stream(oversized)
