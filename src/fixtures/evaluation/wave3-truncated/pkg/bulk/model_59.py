from pydantic import BaseModel


class Item59(BaseModel):
    id: int
    label: str
    quantity: int
