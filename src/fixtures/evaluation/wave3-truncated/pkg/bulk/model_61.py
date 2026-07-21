from pydantic import BaseModel


class Item61(BaseModel):
    id: int
    label: str
    quantity: int
