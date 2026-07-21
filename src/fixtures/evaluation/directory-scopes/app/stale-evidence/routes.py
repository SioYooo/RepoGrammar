from fastapi import APIRouter

router = APIRouter()


@router.get("/alpha")
def alpha_route():
    return {"name": "alpha"}


@router.get("/beta")
def beta_route():
    return {"name": "beta"}


@router.get("/gamma")
def gamma_route():
    return {"name": "gamma"}
