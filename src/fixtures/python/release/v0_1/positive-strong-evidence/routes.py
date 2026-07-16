from fastapi import APIRouter

router = APIRouter()


@router.get("/users")
def list_users():
    return []


@router.get("/accounts")
def list_accounts():
    return []


@router.get("/teams")
def list_teams():
    return []
