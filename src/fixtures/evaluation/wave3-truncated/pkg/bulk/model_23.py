from pydantic import BaseModel


class Item23(BaseModel):
    id: int
    label: str
    quantity: int
