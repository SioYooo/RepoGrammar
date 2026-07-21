from fastapi import APIRouter
from pydantic import BaseModel

router = APIRouter()


class HealthStatus(BaseModel):
    ok: bool


@router.get("/health", response_model=HealthStatus)
def health():
    return HealthStatus(ok=True)
