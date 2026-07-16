from fastapi import APIRouter, Depends
from pydantic import BaseModel

router = APIRouter()


class UserOut(BaseModel):
    id: int


def current_user() -> UserOut:
    return UserOut(id=1)


@router.get("/users", response_model=UserOut)
def list_users(user=Depends(current_user)):
    return user
