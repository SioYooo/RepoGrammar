from pydantic import BaseModel


class Item07(BaseModel):
    id: int
    label: str
    quantity: int
