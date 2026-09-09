! Bounded negative fixture: lookalikes that must never anchor.
module lookalikes
  use testdrive, only : new_unittest, unittest_type
  implicit none

contains

  ! Wrong first dummy type: integer, not the error interface.
  subroutine test_integer_first(error)
    integer, intent(out) :: error
    error = 0
  end subroutine test_integer_first

  ! Function, not subroutine.
  function test_function(error) result(ok)
    type(error_type), allocatable, intent(out) :: error
    logical :: ok
    ok = .true.
  end function test_function

  ! Polymorphic spelling: class, not type.
  subroutine test_class(error)
    class(error_type), allocatable, intent(out) :: error
  end subroutine test_class

  ! Extra attribute beyond intent(out)/allocatable.
  subroutine test_optional(error)
    type(error_type), allocatable, intent(out), optional :: error
  end subroutine test_optional

  ! The only-list omits error_type, so the import proves nothing.
  subroutine test_only_list(error)
    use testdrive, only : new_unittest, unittest_type
    type(error_type), allocatable, intent(out) :: error
  end subroutine test_only_list

  ! Renamed binding: the error_type spelling is not proven accessible.
  subroutine test_renamed(err)
    use testdrive, only : err => error_type
    type(err), allocatable, intent(out) :: err
  end subroutine test_renamed

  ! Not test-prefixed, regardless of the error interface.
  subroutine check_helper(error)
    type(error_type), allocatable, intent(out) :: error
  end subroutine check_helper

end module lookalikes
