from pydantic import BaseModel


class Item12(BaseModel):
    id: int
    label: str
    quantity: int
