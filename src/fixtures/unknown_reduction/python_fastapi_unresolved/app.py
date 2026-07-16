from fastapi import APIRouter, Depends

router = APIRouter()


def make_dependency():
    return object()


@router.get("/users")
def list_users(user=Depends(make_dependency())):
    return {"user": user}
