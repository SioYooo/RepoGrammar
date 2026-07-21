from sqlalchemy import select
from sqlalchemy.orm import Session


class User:
    pass


class UserRepository:
    def list_users(self, session: Session):
        return session.execute(select(User)).scalars().all()

    def get_user(self, session: Session, user_id: int):
        return session.execute(select(User)).scalar_one_or_none()

    def find_user_by_email(self, session: Session, email: str):
        return session.execute(select(User)).scalar_one_or_none()
