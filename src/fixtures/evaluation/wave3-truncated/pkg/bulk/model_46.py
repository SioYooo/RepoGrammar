from pydantic import BaseModel


class Item46(BaseModel):
    id: int
    label: str
    quantity: int
