from pydantic import BaseModel


class Item32(BaseModel):
    id: int
    label: str
    quantity: int
