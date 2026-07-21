from pydantic import BaseModel


class Item43(BaseModel):
    id: int
    label: str
    quantity: int
