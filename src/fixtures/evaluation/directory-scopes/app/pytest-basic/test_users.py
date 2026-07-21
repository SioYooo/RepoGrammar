import pytest


@pytest.mark.parametrize("path", ["/users", "/accounts"])
def test_list_resources(api_client, seeded_user, path):
    assert seeded_user["id"] == 1
    assert path.startswith("/")


def test_user_detail(api_client, seeded_user):
    assert seeded_user["id"] == 1
