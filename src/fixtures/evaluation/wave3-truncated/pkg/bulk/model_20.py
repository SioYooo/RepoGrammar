from pydantic import BaseModel


class Item20(BaseModel):
    id: int
    label: str
    quantity: int
