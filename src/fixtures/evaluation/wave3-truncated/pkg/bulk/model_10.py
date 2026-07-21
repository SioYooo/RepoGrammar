from pydantic import BaseModel


class Item10(BaseModel):
    id: int
    label: str
    quantity: int
