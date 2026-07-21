from pydantic import BaseModel


class Item58(BaseModel):
    id: int
    label: str
    quantity: int
