from fastapi import APIRouter, FastAPI

app = FastAPI()
router = APIRouter()


@router.get("/users")
def list_users():
    return []


@app.post("/users")
def create_user():
    return {}


@router.delete("/users/{user_id}")
def delete_user(user_id: int):
    return {"deleted": user_id}
