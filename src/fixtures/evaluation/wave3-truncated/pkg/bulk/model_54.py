from pydantic import BaseModel


class Item54(BaseModel):
    id: int
    label: str
    quantity: int
