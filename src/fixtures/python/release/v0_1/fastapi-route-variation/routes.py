from fastapi import APIRouter, FastAPI

app = FastAPI()
router = APIRouter()


@router.get("/users")
def router_get_users():
    return []


@router.post("/users")
def router_create_user():
    return {}


@app.post("/users")
def app_create_user():
    return {}


@router.delete("/users/{user_id}")
def router_delete_user(user_id: int):
    return {"deleted": user_id}


@app.delete("/users/{user_id}")
def app_delete_user(user_id: int):
    return {"deleted": user_id}


@app.get("/accounts")
def app_get_accounts():
    return []


@router.head("/health")
def router_head_health():
    return None


@app.head("/health")
def app_head_health():
    return None


@router.options("/users")
def router_options_users():
    return {}


@app.options("/users")
def app_options_users():
    return {}


@router.patch("/users/{user_id}")
def router_patch_user(user_id: int):
    return {"patched": user_id}


@app.patch("/users/{user_id}")
def app_patch_user(user_id: int):
    return {"patched": user_id}


@router.put("/users/{user_id}")
def router_put_user(user_id: int):
    return {"updated": user_id}


@app.put("/users/{user_id}")
def app_put_user(user_id: int):
    return {"updated": user_id}
