from pydantic import BaseModel


class Item60(BaseModel):
    id: int
    label: str
    quantity: int
