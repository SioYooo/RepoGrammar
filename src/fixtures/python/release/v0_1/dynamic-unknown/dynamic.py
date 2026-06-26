import importlib
import sys

from fastapi import APIRouter, Depends
from pydantic import create_model


router = APIRouter()
DynamicUser = create_model("DynamicUser", secret=(str, ...))


def decorator_factory(name: str):
    def inner(function):
        return function
    return inner


def load_plugin(name: str, registry: dict[str, object], extra_path: str):
    sys.path.append(extra_path)
    module = importlib.import_module(name)
    handler = getattr(module, "handle")
    locals()[name]()
    eval(extra_path)
    exec(extra_path)
    compile(extra_path, "<repogrammar>", "exec")
    __import__(name)
    registry[name] = handler
    return getattr(module, "handle")()


@decorator_factory("secret")
def install_patch(target, method):
    setattr(target, method, object())
    return target


@unknown_policy
def guarded_handler():
    return {}


def make_dependency():
    return object()


@router.get("/dynamic")
def dynamic_dependency(current_user=Depends(make_dependency())):
    return {"current_user": current_user}
