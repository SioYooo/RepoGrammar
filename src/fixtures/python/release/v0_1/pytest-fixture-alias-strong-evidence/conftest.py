import pytest as pt
from pytest import fixture as pytest_fixture


@pt.fixture
def client():
    return object()


@pytest_fixture
def db():
    return object()


@pt.fixture
def settings():
    return object()
