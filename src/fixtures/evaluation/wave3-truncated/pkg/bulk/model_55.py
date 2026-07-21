from pydantic import BaseModel


class Item55(BaseModel):
    id: int
    label: str
    quantity: int
