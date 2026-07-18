"""Tiny synthetic FastAPI-style route module for grader unit tests.

Not a real application. Exercises the patch_static_assert grader: the "solved"
state adds a GET /{id}/summary route using the SessionDep dependency; the
unsolved state (this file as committed) lacks it.
"""

from fastapi import APIRouter

router = APIRouter()


@router.get("/{id}")
def read_item(id: int, session):  # SessionDep in the real repo
    return {"id": id}
