from pydantic import BaseModel


class Item56(BaseModel):
    id: int
    label: str
    quantity: int
