from pydantic import BaseModel


class Item29(BaseModel):
    id: int
    label: str
    quantity: int
