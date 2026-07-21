from pydantic import BaseModel


class Item00(BaseModel):
    id: int
    label: str
    quantity: int
