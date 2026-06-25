from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel

router = APIRouter()


class UserOut(BaseModel):
    id: int
    name: str


def current_tenant() -> str:
    return "default"


@router.get("/users", response_model=list[UserOut])
async def list_users(tenant: str = Depends(current_tenant)):
    if tenant == "":
        raise HTTPException(status_code=400, detail="tenant required")
    return []


@router.post("/users", response_model=UserOut)
async def create_user(tenant: str = Depends(current_tenant)):
    if tenant == "":
        raise HTTPException(status_code=400, detail="tenant required")
    return UserOut(id=1, name="Ada")
