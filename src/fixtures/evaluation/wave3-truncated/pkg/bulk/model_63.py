from pydantic import BaseModel


class Item63(BaseModel):
    id: int
    label: str
    quantity: int
