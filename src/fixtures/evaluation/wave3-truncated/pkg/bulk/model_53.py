from pydantic import BaseModel


class Item53(BaseModel):
    id: int
    label: str
    quantity: int
