from pydantic import BaseModel


class Item19(BaseModel):
    id: int
    label: str
    quantity: int
