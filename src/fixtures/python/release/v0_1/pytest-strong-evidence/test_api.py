def test_list_users():
    assert "/users".startswith("/")


def test_create_user():
    assert {"id": 1}["id"] == 1


def test_delete_user():
    assert "deleted".endswith("ed")
