from pydantic import BaseModel


class Item05(BaseModel):
    id: int
    label: str
    quantity: int
