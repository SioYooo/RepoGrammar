from pydantic import BaseModel


class Item22(BaseModel):
    id: int
    label: str
    quantity: int
