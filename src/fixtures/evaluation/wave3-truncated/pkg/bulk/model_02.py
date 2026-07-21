from pydantic import BaseModel


class Item02(BaseModel):
    id: int
    label: str
    quantity: int
