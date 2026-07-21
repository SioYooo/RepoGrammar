from pydantic import BaseModel


class Item36(BaseModel):
    id: int
    label: str
    quantity: int
