from pydantic import BaseModel


class Item51(BaseModel):
    id: int
    label: str
    quantity: int
