from pydantic import BaseModel


class Item17(BaseModel):
    id: int
    label: str
    quantity: int
