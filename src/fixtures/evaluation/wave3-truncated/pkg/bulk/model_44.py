from pydantic import BaseModel


class Item44(BaseModel):
    id: int
    label: str
    quantity: int
