from fastapi import APIRouter

router = APIRouter()
api = router
v1 = api


@v1.get("/users")
def list_users():
    return []


@v1.get("/accounts")
def list_accounts():
    return []


@v1.get("/teams")
def list_teams():
    return []
