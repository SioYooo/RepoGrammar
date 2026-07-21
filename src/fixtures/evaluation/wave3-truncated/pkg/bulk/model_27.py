from pydantic import BaseModel


class Item27(BaseModel):
    id: int
    label: str
    quantity: int
