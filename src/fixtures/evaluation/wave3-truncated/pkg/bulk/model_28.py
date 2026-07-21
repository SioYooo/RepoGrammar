from pydantic import BaseModel


class Item28(BaseModel):
    id: int
    label: str
    quantity: int
