from pydantic import BaseModel


class Item26(BaseModel):
    id: int
    label: str
    quantity: int
