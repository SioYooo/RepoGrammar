import pytest


@pytest.fixture
def api_client():
    return object()


@pytest.fixture
def seeded_user():
    return {"id": 1}
