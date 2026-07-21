from pydantic import BaseModel


class Item64(BaseModel):
    id: int
    label: str
    quantity: int
