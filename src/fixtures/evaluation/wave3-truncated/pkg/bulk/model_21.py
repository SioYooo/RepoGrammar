from pydantic import BaseModel


class Item21(BaseModel):
    id: int
    label: str
    quantity: int
