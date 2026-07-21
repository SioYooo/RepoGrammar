from pydantic import BaseModel


class Item52(BaseModel):
    id: int
    label: str
    quantity: int
