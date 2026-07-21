from pydantic import BaseModel


class Item11(BaseModel):
    id: int
    label: str
    quantity: int
