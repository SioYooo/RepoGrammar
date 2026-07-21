from pydantic import BaseModel


class Item25(BaseModel):
    id: int
    label: str
    quantity: int
