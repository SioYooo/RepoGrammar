"""Acquire/deallocate a fixed Mach service right only. Never send service messages."""
import ctypes
import json


def probe():
    library = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
    port_type = ctypes.c_uint32
    library.bootstrap_look_up.argtypes = [port_type, ctypes.c_char_p, ctypes.POINTER(port_type)]
    library.bootstrap_look_up.restype = ctypes.c_int32
    library.mach_port_deallocate.argtypes = [port_type, port_type]
    library.mach_port_deallocate.restype = ctypes.c_int32
    bootstrap = port_type.in_dll(library, "bootstrap_port").value
    task = port_type.in_dll(library, "mach_task_self_").value
    port = port_type(0)
    result = library.bootstrap_look_up(bootstrap, b"com.apple.SecurityServer", ctypes.byref(port))
    received = port.value != 0
    released = library.mach_port_deallocate(task, port.value) if received else None
    return {"lookup_return": result, "received_right": received, "deallocate_return": released}


if __name__ == "__main__":
    print(json.dumps(probe(), sort_keys=True))
