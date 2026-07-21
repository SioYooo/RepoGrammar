from pydantic import BaseModel


class Item50(BaseModel):
    id: int
    label: str
    quantity: int
