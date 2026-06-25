import importlib
import sys


def decorator_factory(name: str):
    def inner(function):
        return function
    return inner


def load_plugin(name: str, registry: dict[str, object], extra_path: str):
    sys.path.append(extra_path)
    module = importlib.import_module(name)
    handler = getattr(module, "handle")
    registry[name] = handler
    return getattr(module, "handle")()


@decorator_factory("secret")
def install_patch(target, method):
    setattr(target, method, object())
    return target
