from pydantic import BaseModel


class Item34(BaseModel):
    id: int
    label: str
    quantity: int
