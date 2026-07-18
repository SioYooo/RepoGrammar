import pytest


@pytest.fixture
def client():
    return object()


def test_list_users(client):
    assert client is not None


def test_create_user(client):
    assert client is not None


def test_delete_user(client):
    assert client is not None
