from fastapi import APIRouter

router = APIRouter()


@router.get("/lonely")
def lonely_route():
    return {"ok": True}
