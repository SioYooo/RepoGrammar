from pydantic import BaseModel


class Item06(BaseModel):
    id: int
    label: str
    quantity: int
