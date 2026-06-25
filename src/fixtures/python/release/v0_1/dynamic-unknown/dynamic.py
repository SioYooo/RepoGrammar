import importlib


def load_plugin(name: str, registry: dict[str, object]):
    module = importlib.import_module(name)
    handler = getattr(module, "handle")
    registry[name] = handler
    return handler
