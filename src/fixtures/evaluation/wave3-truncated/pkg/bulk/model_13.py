from pydantic import BaseModel


class Item13(BaseModel):
    id: int
    label: str
    quantity: int
