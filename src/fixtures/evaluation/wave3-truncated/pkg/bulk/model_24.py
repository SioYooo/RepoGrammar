from pydantic import BaseModel


class Item24(BaseModel):
    id: int
    label: str
    quantity: int
