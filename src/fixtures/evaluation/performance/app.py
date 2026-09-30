from fastapi import APIRouter

router = APIRouter()


@router.get("/items")
def list_items():
    return []


@router.get("/accounts")
def list_accounts():
    return []


@router.get("/teams")
def list_teams():
    return []
