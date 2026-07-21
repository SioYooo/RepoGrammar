from pydantic import BaseModel


class Item49(BaseModel):
    id: int
    label: str
    quantity: int
