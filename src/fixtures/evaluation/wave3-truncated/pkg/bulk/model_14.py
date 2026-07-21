from pydantic import BaseModel


class Item14(BaseModel):
    id: int
    label: str
    quantity: int
