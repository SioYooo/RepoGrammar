from pydantic import BaseModel


class UserOut(BaseModel):
    id: int
    name: str


class TeamOut(BaseModel):
    id: int
    name: str


class AccountOut(BaseModel):
    id: int
    name: str
