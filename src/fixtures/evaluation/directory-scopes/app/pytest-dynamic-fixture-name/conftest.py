import pytest


fixture_name = "dynamic_client"


@pytest.fixture(name=fixture_name)
def dynamic_client():
    return object()
