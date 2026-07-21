from pydantic import BaseModel


class Item01(BaseModel):
    id: int
    label: str
    quantity: int
