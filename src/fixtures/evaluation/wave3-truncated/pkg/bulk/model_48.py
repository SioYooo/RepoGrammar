from pydantic import BaseModel


class Item48(BaseModel):
    id: int
    label: str
    quantity: int
