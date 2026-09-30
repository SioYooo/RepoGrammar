def test_account(account):
    assert account["id"] == 1


def test_second_account(account):
    assert account["id"] > 0
