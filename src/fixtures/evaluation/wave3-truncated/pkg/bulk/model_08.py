from pydantic import BaseModel


class Item08(BaseModel):
    id: int
    label: str
    quantity: int
